import AppKit
import Foundation
// Usage: swift render.swift scenarios.json outdir
// Draws each frame as a simple browser window (tab strip, address bar, page text lines) and writes PNGs.
let args = CommandLine.arguments
let data = try Data(contentsOf: URL(fileURLWithPath: args[1]))
let root = try JSONSerialization.jsonObject(with: data) as! [[String: Any]]
let out = URL(fileURLWithPath: args[2])
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
let W = 1280, H = 720
func draw(_ text: String, _ x: Double, _ y: Double, _ size: Double, _ bold: Bool, _ color: NSColor, _ ctx: CGContext) {
    let font = NSFont.systemFont(ofSize: size, weight: bold ? .bold : .regular)
    let s = NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color])
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
    s.draw(at: NSPoint(x: x, y: y))
    NSGraphicsContext.restoreGraphicsState()
}
for sc in root {
    let name = sc["name"] as! String
    let frames = sc["frames"] as! [[String: Any]]
    for (i, f) in frames.enumerated() {
        let cs = CGColorSpaceCreateDeviceRGB()
        let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        ctx.translateBy(x: 0, y: CGFloat(H)); ctx.scaleBy(x: 1, y: -1) // y down
        ctx.setFillColor(NSColor.white.cgColor); ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
        ctx.setFillColor(NSColor(white: 0.88, alpha: 1).cgColor); ctx.fill(CGRect(x: 0, y: 0, width: W, height: 36))
        ctx.setFillColor(NSColor(white: 0.96, alpha: 1).cgColor); ctx.fill(CGRect(x: 0, y: 36, width: W, height: 44))
        ctx.setFillColor(NSColor.white.cgColor); ctx.fill(CGRect(x: 90, y: 42, width: 900, height: 30))
        draw(f["title"] as? String ?? "", 20, 10, 13, false, .black, ctx)
        draw(f["url"] as? String ?? "", 104, 48, 15, false, NSColor(white: 0.2, alpha: 1), ctx)
        for l in f["lines"] as! [[String: Any]] {
            draw(l["t"] as! String, l["x"] as! Double, l["y"] as! Double, l["s"] as? Double ?? 16, l["b"] as? Bool ?? false, .black, ctx)
        }
        let image = ctx.makeImage()!
        let rep = NSBitmapImageRep(cgImage: image)
        let png = rep.representation(using: .png, properties: [:])!
        try png.write(to: out.appendingPathComponent("\(name)-\(i + 1).png"))
    }
}
print("rendered")
