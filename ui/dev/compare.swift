// context-capsule — ui: compare — puts each Mac render (left) beside ours (right) with a thin gap, and prints how far apart.
// Build: swiftc -O ui/dev/compare.swift -o /tmp/shared-ui-tools/compare
// Usage: compare <refDir> <ourDir> <outDir>   (pairs <name>.png by name; the right side is scaled to the left's width)
import AppKit

let args = CommandLine.arguments
guard args.count == 4 else { print("usage: compare <refDir> <ourDir> <outDir>"); exit(2) }
let (refDir, ourDir, outDir) = (URL(fileURLWithPath: args[1]), URL(fileURLWithPath: args[2]), URL(fileURLWithPath: args[3]))
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

func load(_ u: URL) -> CGImage? {
    guard let src = CGImageSourceCreateWithURL(u as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(src, 0, nil)
}

// RGBA bytes of an image drawn at w×h.
func pixels(_ img: CGImage, _ w: Int, _ h: Int) -> [UInt8] {
    var buf = [UInt8](repeating: 0, count: w * h * 4)
    let ctx = CGContext(data: &buf, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
    return buf
}

let gap = 16
let names = ((try? FileManager.default.contentsOfDirectory(atPath: refDir.path)) ?? []).filter { $0.hasSuffix(".png") }.sorted()
for name in names {
    guard let ref = load(refDir.appendingPathComponent(name)) else { continue }
    guard let ours = load(ourDir.appendingPathComponent(name)) else { print("\(name): no render of ours"); continue }
    let w = ref.width, h = ref.height
    let oh = Int((Double(ours.height) * Double(w) / Double(ours.width)).rounded()) // ours at the ref's scale
    let H = max(h, oh), W = w * 2 + gap
    let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.setFillColor(CGColor(gray: 0.5, alpha: 1)) // mid-grey gap and padding (the Mac refs have see-through corners)
    ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
    ctx.interpolationQuality = .high
    ctx.draw(ref, in: CGRect(x: 0, y: H - h, width: w, height: h))
    ctx.draw(ours, in: CGRect(x: w + gap, y: H - oh, width: w, height: oh))
    let out = outDir.appendingPathComponent(name)
    guard let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil) else { continue }
    CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
    CGImageDestinationFinalize(dest)

    // How different: share of pixels whose channels differ by more than 24 (same size only).
    var note = "sizes differ (\(w)×\(h) vs \(ours.width)×\(ours.height))"
    if ours.width == w && ours.height == h {
        let a = pixels(ref, w, h), b = pixels(ours, w, h)
        var off = 0
        for i in stride(from: 0, to: a.count, by: 4)
        where abs(Int(a[i]) - Int(b[i])) > 24 || abs(Int(a[i + 1]) - Int(b[i + 1])) > 24 || abs(Int(a[i + 2]) - Int(b[i + 2])) > 24 {
            off += 1
        }
        note = String(format: "%.1f%% pixels differ", Double(off) * 100 / Double(w * h))
    }
    print("\(name): \(note) -> \(out.path)")
}
