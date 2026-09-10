import XCTest
@testable import AgentHUDCore

final class HoverMachineTests: XCTestCase {
    let config = HoverMachine.Config(hoverDelay: 0.4, collapseDelay: 0.2)
    let t0 = Date(timeIntervalSince1970: 1_000)

    func testEnterSchedulesOpenAfterDelay() {
        let t = HoverMachine().reduce(.pointerEntered(at: t0), config: config)
        XCTAssertEqual(t.machine.state, .opening(deadline: t0.addingTimeInterval(0.4)))
        XCTAssertEqual(t.deadline, t0.addingTimeInterval(0.4))
        XCTAssertFalse(t.machine.isOpen)
    }

    func testTimerOpens() {
        let opening = HoverMachine().reduce(.pointerEntered(at: t0), config: config).machine
        let t = opening.reduce(.timerFired(at: t0.addingTimeInterval(0.4)), config: config)
        XCTAssertEqual(t.machine.state, .open)
        XCTAssertNil(t.deadline)
        XCTAssertTrue(t.machine.isOpen)
    }

    func testEarlyTimerIsIgnored() {
        let opening = HoverMachine().reduce(.pointerEntered(at: t0), config: config).machine
        let t = opening.reduce(.timerFired(at: t0.addingTimeInterval(0.1)), config: config)
        XCTAssertEqual(t.machine, opening)
    }

    func testExitWhileOpeningCancels() {
        let opening = HoverMachine().reduce(.pointerEntered(at: t0), config: config).machine
        let t = opening.reduce(.pointerExited(at: t0.addingTimeInterval(0.1)), config: config)
        XCTAssertEqual(t.machine.state, .collapsed)
        XCTAssertNil(t.deadline)
    }

    func testExitFromOpenSchedulesCollapse() {
        let open = HoverMachine(state: .open)
        let t = open.reduce(.pointerExited(at: t0), config: config)
        XCTAssertEqual(t.machine.state, .closing(deadline: t0.addingTimeInterval(0.2)))
        XCTAssertTrue(t.machine.isOpen, "content stays visible while closing")
        let collapsed = t.machine.reduce(.timerFired(at: t0.addingTimeInterval(0.2)), config: config)
        XCTAssertEqual(collapsed.machine.state, .collapsed)
    }

    func testReenterWhileClosingReopensImmediately() {
        let closing = HoverMachine(state: .open).reduce(.pointerExited(at: t0), config: config).machine
        let t = closing.reduce(.pointerEntered(at: t0.addingTimeInterval(0.1)), config: config)
        XCTAssertEqual(t.machine.state, .open)
        XCTAssertNil(t.deadline)
    }

    func testForceEvents() {
        XCTAssertEqual(HoverMachine().reduce(.forceOpen, config: config).machine.state, .open)
        XCTAssertEqual(HoverMachine(state: .open).reduce(.forceCollapse, config: config).machine.state, .collapsed)
    }

    func testZeroDelayStillGoesThroughTimer() {
        let zero = HoverMachine.Config(hoverDelay: 0, collapseDelay: 0)
        let t = HoverMachine().reduce(.pointerEntered(at: t0), config: zero)
        XCTAssertEqual(t.deadline, t0)
        XCTAssertEqual(t.machine.reduce(.timerFired(at: t0), config: zero).machine.state, .open)
    }
}
