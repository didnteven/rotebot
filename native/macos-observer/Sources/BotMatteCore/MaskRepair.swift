/// Restore small enclosed mask holes (such as luminous eyes), without expanding
/// the silhouette or filling gaps connected to the surrounding background.
public enum MaskRepair {
    public static func repair(original: [UInt8], masked: [UInt8], width: Int, height: Int) -> [UInt8] {
        guard width > 0, height > 0, original.count == width * height * 4,
              masked.count == original.count else { return masked }
        let count = width * height
        let foreground = (0..<count).reduce(0) { $0 + (masked[$1 * 4 + 3] >= 128 ? 1 : 0) }
        let maximumHole = max(1, foreground / 50)
        var visited = [Bool](repeating: false, count: count)
        var result = masked
        for start in 0..<count where !visited[start] && masked[start * 4 + 3] < 128 {
            var region = [start]
            visited[start] = true
            var cursor = 0
            var exterior = false
            while cursor < region.count {
                let pixel = region[cursor]
                cursor += 1
                let x = pixel % width, y = pixel / width
                if x == 0 || y == 0 || x == width - 1 || y == height - 1 { exterior = true }
                for next in [x > 0 ? pixel - 1 : -1, x + 1 < width ? pixel + 1 : -1,
                             y > 0 ? pixel - width : -1, y + 1 < height ? pixel + width : -1]
                where next >= 0 && !visited[next] && masked[next * 4 + 3] < 128 {
                    visited[next] = true
                    region.append(next)
                }
            }
            guard !exterior, region.count <= maximumHole else { continue }
            // Both arrays contain premultiplied RGBA. Copy the original color too:
            // changing alpha alone would leave black or dimmed eye pixels.
            for pixel in region {
                for channel in 0..<4 { result[pixel * 4 + channel] = original[pixel * 4 + channel] }
            }
        }
        return result
    }
}
