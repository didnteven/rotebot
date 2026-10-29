import Foundation
import CoreGraphics

// CGImage's raster row order is already top-to-bottom. A CGContext y-flip here
// would reverse the RGBA rows while Vision boxes still refer to the original image.
enum CapturePixels {
    static func rgba(_ image: CGImage) throws -> Data {
        let width = image.width, height = image.height
        guard width > 0, height > 0, width <= 1600, height <= 1600,
              width*height*4 <= CaptureWire.maxPayload-CaptureWire.maxHeader else { throw CaptureWire.Failure.oversized }
        var pixels = Data(count: width*height*4)
        let rendered = pixels.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                          bytesPerRow: width*4, space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard rendered else { throw CaptureWire.Failure.invalid }
        return pixels
    }
}
