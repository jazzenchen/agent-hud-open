import Foundation
import Testing
@testable import AgentHUDSupport

@Test func preservesLargeIntegerCounters() throws {
    let counter: Int64 = 9_007_199_254_740_993
    let value = try JSONDecoder().decode(JSONValue.self, from: Data("{\"tokens\":9007199254740993}".utf8))
    #expect(value == .object(["tokens": .integer(counter)]))
    #expect(String(decoding: try RecordCoding.encoder().encode(value), as: UTF8.self) == "{\"tokens\":9007199254740993}")
}

@Test func recordIdentitiesHaveUnambiguousComponents() {
    #expect(RecordCoding.hash(["ab", "c"]) != RecordCoding.hash(["a", "bc"]))
    #expect(RecordCoding.hash(["", "a"]) != RecordCoding.hash(["a", ""]))
    #expect(RecordCoding.hash([]) == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
}

@Test func datesRoundTripAtMillisecondPrecision() throws {
    struct Sample: Codable, Equatable { var at: Date; var count: Int64 }
    let sample = Sample(at: RecordCoding.date(1_700_000_000_123), count: 9_007_199_254_740_993)
    let value = try JSONValue.from(sample)
    #expect(try value.decode(Sample.self) == sample)
    #expect(RecordCoding.milliseconds(sample.at) == 1_700_000_000_123)
}
