import XCTest
@testable import AgentHUDCore
@testable import AgentHUDDesktop

final class AccountWalletRowTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.en)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    func testPrepaidKnownZeroAndMissingBalanceHaveDifferentLabels() {
        let zero = AccountWalletLabels(wallet: .init(kind: .prepaid, currency: "USD", balance: 0))
        let missing = AccountWalletLabels(wallet: .init(kind: .prepaid, currency: "USD"))
        XCTAssertEqual(zero.name, "Prepaid balance")
        XCTAssertEqual(zero.amounts, "$0.00")
        XCTAssertEqual(missing.amounts, "N/A")
    }

    func testOnDemandKeepsZeroLimitAndEachMissingAmountVisible() {
        let zero = AccountWalletLabels(wallet: .init(kind: .onDemand, currency: "USD", used: 0, limit: 0))
        XCTAssertEqual(zero.name, "On-demand")
        XCTAssertEqual(zero.amounts, "Used $0.00 · Limit $0.00")
        XCTAssertEqual(AccountWalletLabels(wallet: .init(kind: .onDemand, currency: "USD", used: 3)).amounts,
                       "Used $3.00 · Limit N/A")
        XCTAssertEqual(AccountWalletLabels(wallet: .init(kind: .onDemand, currency: "USD", limit: 20)).amounts,
                       "Used N/A · Limit $20.00")
    }

    func testWalletAmountsKeepTheirReportedCurrencyAndDecimals() {
        let wallet = AccountWalletLabels(wallet: .init(kind: .onDemand, currency: "EUR", used: Decimal(string: "2.50"),
                                                      limit: Decimal(string: "12.75")))
        XCTAssertEqual(wallet.amounts, "Used €2.50 · Limit €12.75")
    }
}
