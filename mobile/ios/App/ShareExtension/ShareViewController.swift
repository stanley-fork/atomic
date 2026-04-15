import UIKit
import Social
import UniformTypeIdentifiers

@objc(ShareViewController)
class ShareViewController: UIViewController {

    private let containerView = UIView()
    private let statusLabel = UILabel()
    private let activityIndicator = UIActivityIndicatorView(style: .medium)
    private let iconView = UIImageView()

    override func viewDidLoad() {
        super.viewDidLoad()
        setupUI()
        processSharedContent()
    }

    private func setupUI() {
        view.backgroundColor = UIColor(white: 0, alpha: 0.4)

        containerView.backgroundColor = UIColor(red: 0.118, green: 0.118, blue: 0.118, alpha: 1) // #1e1e1e
        containerView.layer.cornerRadius = 16
        containerView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(containerView)

        activityIndicator.color = .white
        activityIndicator.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(activityIndicator)

        iconView.translatesAutoresizingMaskIntoConstraints = false
        iconView.tintColor = UIColor(red: 0.486, green: 0.228, blue: 0.929, alpha: 1) // #7c3aed
        iconView.contentMode = .scaleAspectFit
        containerView.addSubview(iconView)

        statusLabel.textColor = .white
        statusLabel.font = .systemFont(ofSize: 15, weight: .medium)
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 2
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(statusLabel)

        NSLayoutConstraint.activate([
            containerView.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            containerView.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            containerView.widthAnchor.constraint(equalToConstant: 260),
            containerView.heightAnchor.constraint(equalToConstant: 120),

            activityIndicator.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
            activityIndicator.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 28),

            iconView.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
            iconView.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 24),
            iconView.widthAnchor.constraint(equalToConstant: 28),
            iconView.heightAnchor.constraint(equalToConstant: 28),

            statusLabel.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -16),
            statusLabel.topAnchor.constraint(equalTo: activityIndicator.bottomAnchor, constant: 12),
        ])

        iconView.isHidden = true
        activityIndicator.startAnimating()
        statusLabel.text = "Saving to Atomic..."
    }

    private func processSharedContent() {
        guard SharedConfig.isConfigured else {
            showResult(success: false, message: "Open Atomic and connect\nto a server first")
            return
        }

        guard let items = extensionContext?.inputItems as? [NSExtensionItem] else {
            showResult(success: false, message: "No content to share")
            return
        }

        for item in items {
            guard let attachments = item.attachments else { continue }
            for provider in attachments {
                if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
                    let itemProvider = provider
                    Task { @MainActor [weak self] in
                        do {
                            let item = try await itemProvider.loadItem(forTypeIdentifier: UTType.url.identifier)
                            if let url = item as? URL {
                                self?.ingestURL(url.absoluteString)
                            } else if let urlString = item as? String {
                                self?.ingestURL(urlString)
                            } else {
                                self?.showResult(success: false, message: "Could not read URL")
                            }
                        } catch {
                            self?.showResult(success: false, message: "Could not read URL")
                        }
                    }
                    return
                }
            }
        }

        showResult(success: false, message: "No URL found to share")
    }

    private func ingestURL(_ urlString: String) {
        guard let serverURL = SharedConfig.serverURL,
              let token = SharedConfig.apiToken,
              let baseURL = URL(string: serverURL),
              let endpoint = URL(string: "/api/ingest/url", relativeTo: baseURL) else {
            showResult(success: false, message: "Invalid server configuration")
            return
        }

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let dbId = SharedConfig.databaseId {
            request.setValue(dbId, forHTTPHeaderField: "X-Atomic-Database")
        }

        let body: [String: Any] = [
            "url": urlString,
            "tag_ids": [] as [String]
        ]
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)

        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            DispatchQueue.main.async {
                if let error {
                    self?.showResult(success: false, message: error.localizedDescription)
                    return
                }

                guard let http = response as? HTTPURLResponse else {
                    self?.showResult(success: false, message: "No response from server")
                    return
                }

                if 200..<300 ~= http.statusCode {
                    var title = "Saved"
                    if let data,
                       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                       let t = json["title"] as? String {
                        title = t
                    }
                    self?.showResult(success: true, message: title)
                } else if http.statusCode == 409 {
                    self?.showResult(success: false, message: "URL already saved")
                } else {
                    var message = "Server error (\(http.statusCode))"
                    if let data,
                       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                       let serverError = json["error"] as? String {
                        // Strip the "Ingestion error: " prefix the server tacks on.
                        message = serverError.replacingOccurrences(of: "Ingestion error: ", with: "")
                    }
                    self?.showResult(success: false, message: message)
                }
            }
        }.resume()
    }

    private func showResult(success: Bool, message: String) {
        activityIndicator.stopAnimating()
        activityIndicator.isHidden = true
        iconView.isHidden = false
        iconView.image = UIImage(systemName: success ? "checkmark.circle.fill" : "xmark.circle.fill")
        iconView.tintColor = success
            ? UIColor(red: 0.486, green: 0.228, blue: 0.929, alpha: 1)
            : UIColor.systemRed
        statusLabel.text = message

        DispatchQueue.main.asyncAfter(deadline: .now() + (success ? 1.5 : 2.5)) { [weak self] in
            self?.extensionContext?.completeRequest(returningItems: nil)
        }
    }
}
