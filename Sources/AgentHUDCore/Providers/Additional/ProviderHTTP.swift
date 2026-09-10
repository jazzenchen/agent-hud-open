import Foundation
import Security

/// Credential-bearing requests never follow redirects or use the shared cookie/cache stores.
struct ProviderHTTP: Sendable {
    typealias Transport = @Sendable (URLRequest) async throws -> Data
    var send: Transport = { request in try await requestData(request) }

    func json(_ url: URL, headers: [String: String] = [:], body: ProviderJSON? = nil, timeout: TimeInterval = 12) async throws -> ProviderJSON {
        var request = URLRequest(url: url)
        request.timeoutInterval = timeout
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        for (key, value) in headers { request.setValue(value, forHTTPHeaderField: key) }
        if let body {
            request.httpMethod = "POST"
            request.httpBody = try JSONEncoder().encode(body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        return try ProviderJSON.read(await send(request))
    }

    private static func requestData(_ request: URLRequest) async throws -> Data {
        let config = URLSessionConfiguration.ephemeral
        config.httpShouldSetCookies = false
        config.urlCache = nil
        config.timeoutIntervalForResource = request.timeoutInterval + 1
        let session = URLSession(configuration: config, delegate: Delegate(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let (bytes, response) = try await session.bytes(for: request)
        guard let response = response as? HTTPURLResponse else { throw ProviderFailure.format }
        guard (200..<300).contains(response.statusCode) else {
            throw ProviderHTTPError(status: response.statusCode)
        }
        var data = Data()
        for try await byte in bytes {
            guard data.count < 16 * 1024 * 1024 else { throw ProviderFailure.limit }
            data.append(byte)
        }
        return data
    }

    private final class Delegate: NSObject, URLSessionTaskDelegate, Sendable {
        func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                        newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
            completionHandler(nil)
        }
        func urlSession(_ session: URLSession, task: URLSessionTask, didReceive challenge: URLAuthenticationChallenge,
                        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
            let space = challenge.protectionSpace
            if space.authenticationMethod == NSURLAuthenticationMethodServerTrust,
               space.host == "127.0.0.1", let trust = space.serverTrust {
                completionHandler(.useCredential, URLCredential(trust: trust))
            } else { completionHandler(.performDefaultHandling, nil) }
        }
    }
}

struct ProviderHTTPError: LocalizedError, Sendable {
    let status: Int
    var isAuthentication: Bool { status == 401 || status == 403 }
    var errorDescription: String? {
        isAuthentication
            ? L10n.text("现有登录已失效或无额度读取权限", "Existing sign-in expired or quota access is unavailable")
            : L10n.text("额度服务返回 HTTP \(status)", "Quota service returned HTTP \(status)")
    }
}
