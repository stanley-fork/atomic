//! Fly.io Machines API client

use crate::error::CloudError;
use serde::{Deserialize, Serialize};

const FLY_API_BASE: &str = "https://api.machines.dev/v1";
const FLY_GRAPHQL_URL: &str = "https://api.fly.io/graphql";

/// Fly Machines API client. Cheaply cloneable via Arc.
#[derive(Clone)]
pub struct FlyClient {
    inner: std::sync::Arc<FlyClientInner>,
}

struct FlyClientInner {
    api_token: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct FlyMachine {
    pub id: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct FlyVolume {
    pub id: String,
}

#[derive(Debug, Serialize)]
struct CreateVolumeRequest {
    name: String,
    size_gb: u32,
    region: String,
}

impl FlyClient {
    pub fn new(api_token: String) -> Self {
        Self {
            inner: std::sync::Arc::new(FlyClientInner {
                api_token,
                http: reqwest::Client::new(),
            }),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.inner.api_token)
    }

    /// Create a new Fly app for a customer
    pub async fn create_app(&self, app_name: &str, org_slug: &str) -> Result<(), CloudError> {
        let url = format!("{}/apps", FLY_API_BASE);
        let body = serde_json::json!({
            "app_name": app_name,
            "org_slug": org_slug,
        });

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Create app failed: {body}")));
        }

        Ok(())
    }

    /// Allocate shared IPv4 and IPv6 addresses for an app via GraphQL
    pub async fn allocate_ips(&self, app_name: &str) -> Result<(), CloudError> {
        // Allocate shared IPv4
        let query = serde_json::json!({
            "query": "mutation($input: AllocateIPAddressInput!) { allocateIpAddress(input: $input) { ipAddress { id address type } } }",
            "variables": {
                "input": {
                    "appId": app_name,
                    "type": "shared_v4"
                }
            }
        });

        let resp = self
            .inner.http
            .post(FLY_GRAPHQL_URL)
            .header("Authorization", self.auth_header())
            .json(&query)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Allocate IPv4 failed: {body}")));
        }

        // Allocate IPv6
        let query = serde_json::json!({
            "query": "mutation($input: AllocateIPAddressInput!) { allocateIpAddress(input: $input) { ipAddress { id address type } } }",
            "variables": {
                "input": {
                    "appId": app_name,
                    "type": "v6"
                }
            }
        });

        let resp = self
            .inner.http
            .post(FLY_GRAPHQL_URL)
            .header("Authorization", self.auth_header())
            .json(&query)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Allocate IPv6 failed: {body}")));
        }

        Ok(())
    }

    /// Delete a Fly app and all its resources
    pub async fn delete_app(&self, app_name: &str) -> Result<(), CloudError> {
        let url = format!("{}/apps/{}", FLY_API_BASE, app_name);

        let resp = self
            .inner.http
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Delete app failed: {body}")));
        }

        Ok(())
    }

    /// Create a persistent volume for a customer instance
    pub async fn create_volume(
        &self,
        app_name: &str,
        name: &str,
        size_gb: u32,
        region: &str,
    ) -> Result<FlyVolume, CloudError> {
        let url = format!("{}/apps/{}/volumes", FLY_API_BASE, app_name);
        let body = CreateVolumeRequest {
            name: name.to_string(),
            size_gb,
            region: region.to_string(),
        };

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Create volume failed: {body}")));
        }

        resp.json()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))
    }

    /// Create a Fly Machine with the given configuration
    pub async fn create_machine(
        &self,
        app_name: &str,
        subdomain: &str,
        image: &str,
        volume_id: &str,
        region: &str,
    ) -> Result<FlyMachine, CloudError> {
        let url = format!("{}/apps/{}/machines", FLY_API_BASE, app_name);

        let config = serde_json::json!({
            "name": format!("{}-atomic", subdomain),
            "region": region,
            "config": {
                "image": image,
                "env": {
                    "ATOMIC_STORAGE": "sqlite",
                },
                "guest": {
                    "cpu_kind": "shared",
                    "cpus": 1,
                    "memory_mb": 512
                },
                "mounts": [{
                    "volume": volume_id,
                    "path": "/data"
                }],
                "services": [{
                    "ports": [
                        { "port": 443, "handlers": ["tls", "http"] },
                        { "port": 80, "handlers": ["http"] }
                    ],
                    "protocol": "tcp",
                    "internal_port": 8081
                }],
                "checks": {
                    "health": {
                        "type": "http",
                        "port": 8081,
                        "path": "/health",
                        "interval": "30s",
                        "timeout": "5s"
                    }
                },
                "auto_destroy": false,
                "restart": {
                    "policy": "on-failure",
                    "max_retries": 3
                }
            }
        });

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&config)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Create machine failed: {body}")));
        }

        resp.json()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))
    }

    /// Get the current state of a machine
    pub async fn get_machine(
        &self,
        app_name: &str,
        machine_id: &str,
    ) -> Result<FlyMachine, CloudError> {
        let url = format!("{}/apps/{}/machines/{}", FLY_API_BASE, app_name, machine_id);

        let resp = self
            .inner.http
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Get machine failed: {body}")));
        }

        resp.json()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))
    }

    /// Start a stopped machine
    pub async fn start_machine(
        &self,
        app_name: &str,
        machine_id: &str,
    ) -> Result<(), CloudError> {
        let url = format!(
            "{}/apps/{}/machines/{}/start",
            FLY_API_BASE, app_name, machine_id
        );

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Start machine failed: {body}")));
        }

        Ok(())
    }

    /// Stop a running machine
    pub async fn stop_machine(
        &self,
        app_name: &str,
        machine_id: &str,
    ) -> Result<(), CloudError> {
        let url = format!(
            "{}/apps/{}/machines/{}/stop",
            FLY_API_BASE, app_name, machine_id
        );

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Stop machine failed: {body}")));
        }

        Ok(())
    }

    /// Destroy a machine permanently
    pub async fn destroy_machine(
        &self,
        app_name: &str,
        machine_id: &str,
    ) -> Result<(), CloudError> {
        let url = format!(
            "{}/apps/{}/machines/{}?force=true",
            FLY_API_BASE, app_name, machine_id
        );

        let resp = self
            .inner.http
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Destroy machine failed: {body}")));
        }

        Ok(())
    }

    /// Destroy a volume permanently
    pub async fn destroy_volume(
        &self,
        app_name: &str,
        volume_id: &str,
    ) -> Result<(), CloudError> {
        let url = format!(
            "{}/apps/{}/volumes/{}",
            FLY_API_BASE, app_name, volume_id
        );

        let resp = self
            .inner.http
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Destroy volume failed: {body}")));
        }

        Ok(())
    }

    /// Update a machine's image (for auto-updates).
    /// Fetches the existing config first and merges in the new image to avoid
    /// wiping env vars, mounts, services, and health checks.
    pub async fn update_machine_image(
        &self,
        app_name: &str,
        machine_id: &str,
        image: &str,
    ) -> Result<(), CloudError> {
        let url = format!("{}/apps/{}/machines/{}", FLY_API_BASE, app_name, machine_id);

        // Fetch existing machine config
        let resp = self
            .inner.http
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Get machine for update failed: {body}")));
        }

        let mut machine: serde_json::Value = resp.json().await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        // Update only the image in the existing config
        if let Some(config) = machine.get_mut("config") {
            config["image"] = serde_json::Value::String(image.to_string());
        }

        // Send update with full config
        let update_body = serde_json::json!({
            "config": machine["config"],
        });

        let resp = self
            .inner.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&update_body)
            .send()
            .await
            .map_err(|e| CloudError::Fly(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Fly(format!("Update machine failed: {body}")));
        }

        Ok(())
    }
}
