import XCTest
@testable import BotMatteCore

final class MaskRepairTests: XCTestCase {
    func testRestoresBothEyesAndOriginalColorWithoutFillingExteriorGap() {
        let width = 24, height = 24
        var original = [UInt8](repeating: 255, count: width * height * 4)
        var masked = [UInt8](repeating: 0, count: original.count)
        for y in 2..<22 {
            for x in 2..<22 {
                let pixel = (y * width + x) * 4
                for channel in 0..<3 { original[pixel + channel] = 30; masked[pixel + channel] = 30 }
                masked[pixel + 3] = 255
            }
        }
        let eyes = [7 * width + 8, 7 * width + 15]
        for eye in eyes {
            for channel in 0..<3 { original[eye * 4 + channel] = 240 }
            masked[eye * 4 + 3] = eye == eyes[0] ? 0 : 64
        }
        // An open gap remains background, even inside the character's bounds.
        for y in 15..<24 { masked[(y * width + 12) * 4 + 3] = 0 }
        let repaired = MaskRepair.repair(original: original, masked: masked, width: width, height: height)
        for eye in eyes { XCTAssertEqual(Array(repaired[eye * 4..<eye * 4 + 4]), [240, 240, 240, 255]) }
        XCTAssertEqual(repaired[3], 0)
        XCTAssertEqual(repaired[(17 * width + 12) * 4 + 3], 0)
    }

    func testPreservesLargeEnclosedGapsAndOriginalTransparency() {
        let width = 20, height = 20
        var masked = [UInt8](repeating: 255, count: width * height * 4)
        for y in 5..<15 {
            for x in 5..<15 { masked[(y * width + x) * 4 + 3] = 0 }
        }
        XCTAssertEqual(MaskRepair.repair(original: [UInt8](repeating: 255, count: masked.count), masked: masked, width: width, height: height), masked)
        var original = masked
        masked[(2 * width + 2) * 4 + 3] = 0
        original[(2 * width + 2) * 4 + 3] = 0
        XCTAssertEqual(MaskRepair.repair(original: original, masked: masked, width: width, height: height)[(2 * width + 2) * 4 + 3], 0)
    }
}
