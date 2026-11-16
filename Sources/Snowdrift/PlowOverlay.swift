import AppKit
import QuartzCore

/// The snowplow's body: a small window that rides along the bottom of the screen with the blade of the simulated plow.
/// The snow it scrapes is the simulation's; this only draws the machine.
final class PlowOverlay {
    private static let size = CGSize(width: 420, height: 150)
    let window: OverlayWindow
    private let view = PlowView(frame: CGRect(origin: .zero, size: PlowOverlay.size))
    private var shown = false

    init(screen: NSScreen) {
        window = OverlayWindow(screen: screen)
        window.contentView = view
        window.setFrame(CGRect(origin: screen.frame.origin, size: Self.size), display: false)
    }

    /// Follows the plow, taking the level, fading and screenshot setting of the main overlay.
    func update(_ plow: SnowSimulation.PlowState?, screen: NSScreen, like main: OverlayWindow) {
        guard let plow, main.isVisible else {
            if shown {
                window.orderOut(nil)
                shown = false
            }
            return
        }
        view.p = PlowView.Snapshot(dir: CGFloat(plow.dir), tip: CGFloat(plow.tip), pushed: CGFloat(plow.pushed), dumped: plow.dumped)
        window.setFrameOrigin(CGPoint(x: screen.frame.minX + CGFloat(plow.tip) - view.originX, y: screen.frame.minY))
        window.level = main.level
        window.alphaValue = main.alphaValue
        window.showsInScreenshots = main.showsInScreenshots
        view.needsDisplay = true
        if !shown {
            window.orderFrontRegardless()
            shown = true
        }
    }

    func close() {
        window.orderOut(nil)
        shown = false
    }
}

private final class PlowView: NSView {
    struct Snapshot {
        var dir: CGFloat
        var tip: CGFloat
        var pushed: CGFloat
        var dumped: Bool
    }

    var p = Snapshot(dir: 1, tip: 0, pushed: 0, dumped: false)
    /// Where the blade is inside the window.
    let originX: CGFloat = 210

    override var isOpaque: Bool { false }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        draw(ctx)
    }

    func draw(_ ctx: CGContext) {
        // the heap of snow in front of the blade
        let moundH = 12 + 58 * (1 - exp(-p.pushed / 15000))
        let moundW = 50 + moundH * 0.8
        if !p.dumped && p.pushed > 0 {
            let f = p.dir
            let endX = originX + f * moundW
            let endH = min(moundH, 24)
            ctx.beginPath()
            ctx.move(to: CGPoint(x: originX, y: 0))
            ctx.addLine(to: CGPoint(x: originX, y: moundH))
            ctx.addCurve(to: CGPoint(x: endX, y: endH), control1: CGPoint(x: originX + f * moundW * 0.35, y: moundH * 1.02),
                         control2: CGPoint(x: originX + f * moundW * 0.7, y: moundH * 0.5))
            ctx.addLine(to: CGPoint(x: endX, y: 0))
            ctx.closePath()
            ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.97))
            ctx.fillPath()
        }
        ctx.saveGState()
        ctx.translateBy(x: originX, y: 0)
        if p.dir < 0 { ctx.scaleBy(x: -1, y: 1) }
        ctx.scaleBy(x: 1.25, y: 1.25)
        let t = CACurrentMediaTime()
        func col(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) { ctx.setFillColor(CGColor(red: r, green: g, blue: b, alpha: a)) }
        func rr(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ rad: CGFloat) {
            ctx.addPath(CGPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerWidth: rad, cornerHeight: rad, transform: nil))
            ctx.fillPath()
        }
        // тень под машиной
        col(0, 0, 0, 0.28)
        ctx.fillEllipse(in: CGRect(x: -136, y: -3, width: 142, height: 9))
        // дым из выхлопной трубы (уходит назад, машина едет вперёд)
        for i in 0..<5 {
            let ph = CGFloat((t * 0.7 + Double(i) / 5).truncatingRemainder(dividingBy: 1))
            col(0.82, 0.82, 0.85, 0.38 * (1 - ph))
            let r = 2.5 + ph * 7
            ctx.fillEllipse(in: CGRect(x: -70 - ph * 26 - r, y: 82 + ph * 22 - r, width: r * 2, height: r * 2))
        }
        // рама и задний мост
        col(0.16, 0.16, 0.18); rr(-132, 12, 118, 14, 3)
        col(0.3, 0.3, 0.33);   rr(-132, 22, 118, 3, 1.5)
        // кузов-пескоразбрасыватель: бункер
        col(0.92, 0.48, 0.08)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: -130, y: 26)); ctx.addLine(to: CGPoint(x: -131, y: 56))
        ctx.addLine(to: CGPoint(x: -68, y: 50)); ctx.addLine(to: CGPoint(x: -68, y: 26))
        ctx.closePath(); ctx.fillPath()
        col(0.74, 0.34, 0.05); ctx.fill(CGRect(x: -129, y: 34, width: 61, height: 4))   // полоса по борту
        col(1.0, 0.7, 0.3);    ctx.fill(CGRect(x: -131, y: 53, width: 63, height: 3))    // верхняя кромка
        for i in 0..<6 {                                                                   // сигнальная полоса сзади
            col(i % 2 == 0 ? 0.98 : 0.1, i % 2 == 0 ? 0.85 : 0.1, i % 2 == 0 ? 0.1 : 0.1)
            ctx.fill(CGRect(x: -131, y: 27 + CGFloat(i) * 4.6, width: 9, height: 4.6))
        }
        // снежная шапка на бункере
        col(1, 1, 1, 0.97); rr(-128, 54, 58, 5, 2.5)
        // кабина
        col(0.96, 0.52, 0.1); rr(-66, 26, 42, 46, 7)
        col(0.8, 0.38, 0.06); rr(-70, 67, 50, 7, 3)                                        // крыша
        col(1, 1, 1, 0.97);   rr(-68, 72, 46, 5, 2.5)                                      // снег на крыше
        col(0.13, 0.15, 0.18); rr(-61, 38, 32, 27, 5)                                      // рамка окна
        col(0.55, 0.78, 0.93); rr(-59, 40, 28, 23, 4)                                      // стекло
        col(1, 1, 1, 0.33)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: -57, y: 41)); ctx.addLine(to: CGPoint(x: -49, y: 41))
        ctx.addLine(to: CGPoint(x: -41, y: 62)); ctx.addLine(to: CGPoint(x: -49, y: 62))
        ctx.closePath(); ctx.fillPath()                                                    // блик
        ctx.setStrokeColor(CGColor(red: 0.1, green: 0.1, blue: 0.12, alpha: 1)); ctx.setLineWidth(1.4)
        ctx.move(to: CGPoint(x: -36, y: 41)); ctx.addLine(to: CGPoint(x: -46, y: 52)); ctx.strokePath()   // дворник
        col(0.2, 0.2, 0.22); rr(-39, 32, 10, 3, 1.5)                                       // ручка двери
        // капот и решётка
        col(0.98, 0.58, 0.14); rr(-26, 26, 26, 23, 5)
        col(0.8, 0.38, 0.06);  ctx.fill(CGRect(x: -24, y: 36, width: 22, height: 2))
        col(0.12, 0.12, 0.14); rr(-5, 28, 6, 18, 2)
        for i in 0..<3 { col(0.5, 0.5, 0.55); ctx.fill(CGRect(x: -4, y: 31 + CGFloat(i) * 5, width: 4, height: 1.5)) }
        // фара и конус света
        let glow = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                              colors: [CGColor(red: 1, green: 0.95, blue: 0.6, alpha: 0.55), CGColor(red: 1, green: 0.95, blue: 0.6, alpha: 0)] as CFArray,
                              locations: [0, 1])!
        ctx.drawRadialGradient(glow, startCenter: CGPoint(x: 3, y: 41), startRadius: 1, endCenter: CGPoint(x: 28, y: 36), endRadius: 28, options: [])
        col(1, 0.96, 0.7); ctx.fillEllipse(in: CGRect(x: -3, y: 38, width: 6, height: 6))
        // бампер
        col(0.16, 0.16, 0.18); rr(-8, 21, 14, 6, 2)
        // гидроцилиндр и рычаг отвала
        ctx.setStrokeColor(CGColor(red: 0.2, green: 0.2, blue: 0.22, alpha: 1)); ctx.setLineWidth(4)
        ctx.move(to: CGPoint(x: -22, y: 22)); ctx.addLine(to: CGPoint(x: 2, y: 20)); ctx.strokePath()
        ctx.setStrokeColor(CGColor(red: 0.72, green: 0.72, blue: 0.76, alpha: 1)); ctx.setLineWidth(2)
        ctx.move(to: CGPoint(x: -14, y: 28)); ctx.addLine(to: CGPoint(x: 5, y: 38)); ctx.strokePath()
        // отвал: изогнутая жёлтая пластина, чёрная резиновая кромка, болты, усилители
        col(0.96, 0.78, 0.1)
        ctx.beginPath()
        ctx.move(to: CGPoint(x: -4, y: 9))
        ctx.addQuadCurve(to: CGPoint(x: 8, y: 6), control: CGPoint(x: 3, y: 5))
        ctx.addQuadCurve(to: CGPoint(x: 12, y: 58), control: CGPoint(x: 16, y: 32))
        ctx.addQuadCurve(to: CGPoint(x: -8, y: 62), control: CGPoint(x: 2, y: 63))
        ctx.closePath(); ctx.fillPath()
        col(1, 0.92, 0.5, 0.75)                                                            // блик на пластине
        ctx.beginPath()
        ctx.move(to: CGPoint(x: -3, y: 16)); ctx.addLine(to: CGPoint(x: 0, y: 16))
        ctx.addLine(to: CGPoint(x: 4, y: 56)); ctx.addLine(to: CGPoint(x: 0, y: 58))
        ctx.closePath(); ctx.fillPath()
        col(0.78, 0.58, 0.05)                                                              // усиливающие рёбра
        for y: CGFloat in [22, 36, 50] { ctx.fill(CGRect(x: -2, y: y, width: 13, height: 2)) }
        col(0.1, 0.1, 0.11); rr(-5, 3, 14, 5, 1.5)                                         // резиновая кромка
        col(0.42, 0.42, 0.46)
        for y: CGFloat in [18, 33, 48] { ctx.fillEllipse(in: CGRect(x: 4, y: y, width: 2.6, height: 2.6)) }
        // брызги снега от отвала
        if p.pushed > 0 {
            for i in 0..<7 {
                let ph = CGFloat((t * 1.7 + Double(i) / 7).truncatingRemainder(dividingBy: 1))
                col(1, 1, 1, 0.85 * (1 - ph))
                let r = 1 + 2.4 * (1 - ph)
                ctx.fillEllipse(in: CGRect(x: 10 + ph * 24 - r, y: 20 + sin(CGFloat(i) * 2.1) * 10 + ph * 38 - r, width: r * 2, height: r * 2))
            }
        }
        // колёса: протектор и диск вращаются, пока машина едет
        let spin = -(p.tip * p.dir) / 14
        for wx: CGFloat in [-44, -104] {
            ctx.saveGState()
            ctx.translateBy(x: wx, y: 14)
            ctx.rotate(by: spin)
            col(0.08, 0.08, 0.09); ctx.fillEllipse(in: CGRect(x: -14, y: -14, width: 28, height: 28))
            col(0.2, 0.2, 0.22)
            for k in 0..<10 {
                ctx.saveGState()
                ctx.rotate(by: CGFloat(k) * .pi / 5)
                ctx.fill(CGRect(x: -1.6, y: 11.2, width: 3.2, height: 3.4))
                ctx.restoreGState()
            }
            col(0.62, 0.63, 0.67); ctx.fillEllipse(in: CGRect(x: -7, y: -7, width: 14, height: 14))
            col(0.4, 0.4, 0.44);   ctx.fillEllipse(in: CGRect(x: -2.5, y: -2.5, width: 5, height: 5))
            col(0.32, 0.32, 0.36)
            for k in 0..<5 {
                let a = CGFloat(k) * 2 * .pi / 5
                ctx.fillEllipse(in: CGRect(x: cos(a) * 4.6 - 1, y: sin(a) * 4.6 - 1, width: 2, height: 2))
            }
            ctx.restoreGState()
            ctx.setStrokeColor(CGColor(red: 0.8, green: 0.38, blue: 0.06, alpha: 1)); ctx.setLineWidth(3.5)   // арка крыла
            ctx.addArc(center: CGPoint(x: wx, y: 14), radius: 17, startAngle: 0.08 * .pi, endAngle: 0.92 * .pi, clockwise: false)
            ctx.strokePath()
        }
        // выхлопная труба
        col(0.25, 0.25, 0.28); rr(-72, 50, 5, 34, 2)
        col(0.12, 0.12, 0.14); rr(-73, 82, 7, 4, 1.5)
        // мигалка с отблеском
        let on = Int(t * 4) % 2 == 0
        col(0.2, 0.2, 0.22); rr(-52, 74, 12, 3, 1)
        if on {
            let g2 = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                                colors: [CGColor(red: 1, green: 0.55, blue: 0.1, alpha: 0.55), CGColor(red: 1, green: 0.55, blue: 0.1, alpha: 0)] as CFArray,
                                locations: [0, 1])!
            ctx.drawRadialGradient(g2, startCenter: CGPoint(x: -46, y: 82), startRadius: 0, endCenter: CGPoint(x: -46, y: 82), endRadius: 22, options: [])
        }
        col(1, on ? 0.55 : 0.28, 0.1, on ? 1 : 0.6)
        ctx.fillEllipse(in: CGRect(x: -51, y: 76, width: 10, height: 10))
        ctx.restoreGState()
    }
}
