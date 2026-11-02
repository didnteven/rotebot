import Foundation
import BotMatteCore
import Vision
import CoreImage
import ImageIO
import UniformTypeIdentifiers
import Darwin

// One encoded image in, one RGBA PNG out. No file paths, screen access, or API key.
// Keep original extent so all poses retain their padding and registration.
func run() throws {
    var input = Data()
    while let chunk = try FileHandle.standardInput.read(upToCount: 65536), !chunk.isEmpty {
        guard input.count + chunk.count <= 8 * 1024 * 1024 else { throw MatteError.invalid }
        input.append(chunk)
    }
    guard let source = CGImageSourceCreateWithData(input as CFData, nil),
          let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
          let width = properties[kCGImagePropertyPixelWidth] as? Int,
          let height = properties[kCGImagePropertyPixelHeight] as? Int,
          width > 0, height > 0, width <= 4096, height <= 4096,
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { throw MatteError.invalid }
    let handler = VNImageRequestHandler(cgImage: image, options: [:])
    let request = VNGenerateForegroundInstanceMaskRequest()
    try handler.perform([request])
    guard let observation = request.results?.first, !observation.allInstances.isEmpty else { throw MatteError.noSubject }
    let masked = try observation.generateMaskedImage(ofInstances: observation.allInstances, from: handler, croppedToInstancesExtent: false)
    let context = CIContext(options: [.cacheIntermediates: false])
    let ciImage = CIImage(cvPixelBuffer: masked)
    guard let rgba = context.createCGImage(ciImage, from: ciImage.extent, format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB)) else { throw MatteError.invalid }
    let originalPixels = try pixels(image, width: width, height: height)
    let maskedPixels = try pixels(rgba, width: width, height: height)
    let repaired = MaskRepair.repair(original: originalPixels, masked: maskedPixels, width: width, height: height)
    guard let provider = CGDataProvider(data: Data(repaired) as CFData),
          let output = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                               bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                               bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                               provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    else { throw MatteError.invalid }
    let data = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { throw MatteError.invalid }
    CGImageDestinationAddImage(destination, output, nil)
    guard CGImageDestinationFinalize(destination), data.length <= 8 * 1024 * 1024 else { throw MatteError.invalid }
    try FileHandle.standardOutput.write(contentsOf: data as Data)
}
func pixels(_ image: CGImage, width: Int, height: Int) throws -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    try bytes.withUnsafeMutableBytes { buffer in
        guard let context = CGContext(data: buffer.baseAddress, width: width, height: height,
                                      bitsPerComponent: 8, bytesPerRow: width * 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { throw MatteError.invalid }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    return bytes
}
enum MatteError: Error { case invalid, noSubject }
signal(SIGPIPE, SIG_IGN)
do { try autoreleasepool { try run() } }
catch { exit(1) } // The host supplies a plain-language error, never framework details.
