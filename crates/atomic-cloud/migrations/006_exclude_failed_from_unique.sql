-- Allow re-subscription and subdomain reuse for failed instances
DROP INDEX idx_instances_active_customer;
DROP INDEX idx_instances_active_subdomain;
CREATE UNIQUE INDEX idx_instances_active_customer ON instances(customer_id) WHERE status NOT IN ('destroyed', 'failed');
CREATE UNIQUE INDEX idx_instances_active_subdomain ON instances(subdomain) WHERE status NOT IN ('destroyed', 'failed');
