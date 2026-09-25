import XCTest
@testable import AgentHUDDesktop

final class AgentCardsTests: XCTestCase {
    @MainActor
    func testRowsHoldThreeCardsAndNeverLeaveOneAlone() {
        let sizes = (1...10).map { AgentCards.rows(Array(0..<$0)).map(\.count) }
        XCTAssertEqual(sizes, [[1], [2], [3], [2, 2], [3, 2], [3, 3], [3, 2, 2], [3, 3, 2], [3, 3, 3], [3, 3, 2, 2]])
        XCTAssertEqual(AgentCards.rows(Array(0..<5)).flatMap { $0 }, Array(0..<5), "cards keep their order")
    }
}
