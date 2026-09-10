import Foundation

/// Official GET /user/balance. Monetary strings are decoded as decimals, never binary floats.
public struct DeepSeekBalance: Decodable, Sendable {
    public let isAvailable: Bool
    public let balances: [AccountBalance]

    private enum CodingKeys: String, CodingKey {
        case isAvailable = "is_available", balances = "balance_infos"
    }
    private struct Info: Decodable {
        let currency: String
        let total_balance: String
        let granted_balance: String
        let topped_up_balance: String
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        isAvailable = try values.decode(Bool.self, forKey: .isAvailable)
        balances = try values.decode([Info].self, forKey: .balances).map { info in
            func decimal(_ text: String) throws -> Decimal {
                guard text.range(of: #"^-?[0-9]+(?:\.[0-9]+)?$"#, options: .regularExpression) != nil,
                      let amount = Decimal(string: text, locale: Locale(identifier: "en_US_POSIX")), !amount.isNaN else {
                    throw DecodingError.dataCorruptedError(forKey: .balances, in: values, debugDescription: "Invalid balance amount")
                }
                return amount
            }
            return try AccountBalance(currency: info.currency, total: decimal(info.total_balance),
                                      granted: decimal(info.granted_balance), toppedUp: decimal(info.topped_up_balance))
        }
    }
}
