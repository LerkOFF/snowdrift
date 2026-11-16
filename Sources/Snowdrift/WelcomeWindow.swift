import AppKit
import QuartzCore

/// Shown on first launch. Getting started crumbles the window into snow that falls onto the desktop.
final class WelcomeController: NSObject {
    /// Receives the pieces of the window as it crumbles, in global screen coordinates.
    var scatter: ([ScatterPiece]) -> Void = { _ in }
    var finished: () -> Void = {}
    /// Called instead of `scatter` when the window can't be captured and just closes.
    var skipped: () -> Void = {}

    private var window: NSWindow?
    private var crumble: Crumble?

    func show() {
        let size = CGSize(width: 460, height: 460)
        let window = WelcomeWindow(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.level = .floating
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.onCancel = { [weak self] in self?.getStarted() }

        let content = WelcomeView(frame: CGRect(origin: .zero, size: size))
        content.button.target = self
        content.button.action = #selector(getStarted)
        window.contentView = content
        window.center()
        self.window = window

        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    @objc private func getStarted() {
        guard crumble == nil, let window, let view = window.contentView as? WelcomeView else { return }
        view.button.isEnabled = false
        guard let crumble = Crumble(view: view) else {
            window.orderOut(nil)
            skipped()
            finished()
            return
        }
        self.crumble = crumble
        let windowOrigin = window.frame.origin
        crumble.start { [weak self] cells in
            guard let self else { return }
            let center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
            self.scatter(cells.map { cell in
                let x = Float(windowOrigin.x + cell.point.x)
                let y = Float(windowOrigin.y + cell.point.y)
                let outward = Float((cell.point.x - center.x) / view.bounds.width)
                return ScatterPiece(
                    position: SIMD2(x, y),
                    velocity: SIMD2(outward * 140 + .random(in: -70...70), .random(in: 30...150)),
                    color: cell.color
                )
            })
            window.invalidateShadow()
        } completion: { [weak self] in
            window.orderOut(nil)
            self?.crumble = nil
            self?.window = nil
            self?.finished()
        }
    }
}

private final class WelcomeWindow: NSWindow {
    var onCancel: () -> Void = {}

    override var canBecomeKey: Bool { true }

    override func cancelOperation(_ sender: Any?) {
        onCancel()
    }
}

/// Erases a view from the top down along a ragged, soft-edged front and hands each bit it passes,
/// with its colour, to `pieces`, so the window seems to fall apart into the snow.
private final class Crumble {
    struct Cell {
        let point: CGPoint
        let color: SIMD3<Float>
        let delay: Double
    }

    private let view: NSView
    /// Spacing of the pieces that fly off.
    private let cell: CGFloat = 6
    /// Spacing of the mask: fine enough that its edge reads as a line, not as squares.
    private let grain: CGFloat = 2
    /// Seconds over which a spot of the mask fades out once the front reaches it.
    private let feather = 0.05
    private let columns: Int
    private let rows: Int
    /// When the front reaches each mask pixel, top row first.
    private let front: [Double]
    private var cells: [Cell]
    private var next = 0
    private var pixels: [UInt8]
    private let mask = CALayer()
    private var timer: Timer?
    private var started: CFTimeInterval = 0
    private let duration: Double

    init?(view: NSView) {
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: rep)
        self.view = view
        let bounds = view.bounds
        columns = Int((bounds.width / grain).rounded(.up))
        rows = Int((bounds.height / grain).rounded(.up))
        pixels = Array(repeating: 255, count: columns * rows * 4)

        var front = [Double](repeating: 0, count: columns * rows)
        for row in 0..<rows {
            for column in 0..<columns {
                let point = CGPoint(x: (CGFloat(column) + 0.5) * grain, y: bounds.height - (CGFloat(row) + 0.5) * grain)
                front[row * columns + column] = Crumble.arrival(at: point, in: bounds)
            }
        }
        self.front = front

        let scale = CGFloat(rep.pixelsWide) / bounds.width
        var cells: [Cell] = []
        for y in stride(from: cell / 2, to: bounds.height, by: cell) {
            for x in stride(from: cell / 2, to: bounds.width, by: cell) {
                let point = CGPoint(x: x, y: y)
                // Bitmap rows run top-down; the view's run bottom-up.
                let px = min(Int(x * scale), rep.pixelsWide - 1)
                let py = min(Int((bounds.height - y) * scale), rep.pixelsHigh - 1)
                guard let sample = rep.colorAt(x: px, y: py)?.usingColorSpace(.sRGB), sample.alphaComponent > 0.1 else { continue }
                cells.append(Cell(
                    point: point,
                    color: SIMD3(Float(sample.redComponent), Float(sample.greenComponent), Float(sample.blueComponent)),
                    delay: Crumble.arrival(at: point, in: bounds)
                ))
            }
        }
        cells.sort { $0.delay < $1.delay }
        self.cells = cells
        duration = (front.max() ?? 0) + feather
    }

    func start(pieces: @escaping ([Cell]) -> Void, completion: @escaping () -> Void) {
        view.wantsLayer = true
        mask.frame = view.bounds
        mask.contentsGravity = .resize
        view.layer?.mask = mask
        updateMask(at: 0)
        started = CACurrentMediaTime()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }
            let elapsed = CACurrentMediaTime() - self.started
            var batch: [Cell] = []
            while self.next < self.cells.count, self.cells[self.next].delay <= elapsed {
                batch.append(self.cells[self.next])
                self.next += 1
            }
            self.updateMask(at: elapsed)
            if !batch.isEmpty { pieces(batch) }
            if elapsed >= self.duration {
                timer.invalidate()
                completion()
            }
        }
    }

    /// The front sweeps down in about half a second, held back here and there by smooth noise
    /// and roughened by a fine grain so its edge looks like crumbling snow.
    private static func arrival(at point: CGPoint, in bounds: CGRect) -> Double {
        let fromTop = 1 - Double(point.y / bounds.height)
        let x = Double(point.x), y = Double(point.y)
        let broad = valueNoise(x / 46, y / 46) * 0.65 + valueNoise(x / 15 + 7.3, y / 15 + 2.1) * 0.35
        return fromTop * 0.5 + broad * 0.2 + hash(Int(x * 1.7), Int(y * 1.7)) * 0.035
    }

    private func updateMask(at time: Double) {
        for i in 0..<front.count {
            let a = UInt8(max(0, min(1, (front[i] - time) / feather)) * 255)
            pixels[i * 4] = a
            pixels[i * 4 + 1] = a
            pixels[i * 4 + 2] = a
            pixels[i * 4 + 3] = a
        }
        guard let provider = CGDataProvider(data: Data(pixels) as CFData),
              let image = CGImage(
                width: columns,
                height: rows,
                bitsPerComponent: 8,
                bitsPerPixel: 32,
                bytesPerRow: columns * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider,
                decode: nil,
                shouldInterpolate: true,
                intent: .defaultIntent
              )
        else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        mask.contents = image
        CATransaction.commit()
    }

    private static func valueNoise(_ x: Double, _ y: Double) -> Double {
        let x0 = Int(floor(x)), y0 = Int(floor(y))
        let fx = x - floor(x), fy = y - floor(y)
        let u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy)
        let top = hash(x0, y0) + (hash(x0 + 1, y0) - hash(x0, y0)) * u
        let bottom = hash(x0, y0 + 1) + (hash(x0 + 1, y0 + 1) - hash(x0, y0 + 1)) * u
        return top + (bottom - top) * v
    }

    private static func hash(_ x: Int, _ y: Int) -> Double {
        var n = UInt32(truncatingIfNeeded: x &* 374_761_393 &+ y &* 668_265_263)
        n = (n ^ (n >> 13)) &* 1_274_126_177
        n ^= n >> 16
        return Double(n & 0xFFFF) / 65535
    }
}

/// Drawn rather than built from layer properties, so `cacheDisplay` captures exactly what is on screen.
/// A round window: a night sky with a drift along its bottom edge and a snowflake in the middle.
private final class WelcomeView: NSView {
    let button = GetStartedButton()
    private let ink = NSColor(srgbRed: 0.96, green: 0.98, blue: 1, alpha: 1)
    private let muted = NSColor(srgbRed: 0.72, green: 0.8, blue: 0.9, alpha: 1)

    override init(frame: NSRect) {
        super.init(frame: frame)

        let flake = SnowflakeView()
        flake.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([flake.widthAnchor.constraint(equalToConstant: 76), flake.heightAnchor.constraint(equalToConstant: 76)])

        let title = NSTextField(labelWithAttributedString: NSAttributedString(
            string: tr("welcome.title"),
            attributes: [
                .font: NSFont.systemFont(ofSize: 25, weight: .light),
                .foregroundColor: ink,
                .paragraphStyle: Self.paragraph(lineHeight: 1.0),
            ]
        ))
        title.preferredMaxLayoutWidth = 280

        let body = NSTextField(wrappingLabelWithString: tr("welcome.mac"))
        body.attributedStringValue = NSAttributedString(string: body.stringValue, attributes: [
            .font: NSFont.systemFont(ofSize: 12.5),
            .foregroundColor: muted,
            .paragraphStyle: Self.paragraph(lineHeight: 1.28),
        ])
        body.preferredMaxLayoutWidth = 270
        // Selectable labels swap in a field editor on click, which re-lays the text out without its paragraph style.
        title.isSelectable = false
        body.isSelectable = false

        let stack = NSStackView(views: [flake, title, body, button])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 12
        stack.setCustomSpacing(8, after: flake)
        stack.setCustomSpacing(22, after: body)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor, constant: 12),
            stack.widthAnchor.constraint(lessThanOrEqualToConstant: 290),
        ])
    }

    required init?(coder: NSCoder) { nil }

    override var mouseDownCanMoveWindow: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let disc = bounds.insetBy(dx: 1, dy: 1)
        let circle = CGPath(ellipseIn: disc, transform: nil)
        ctx.saveGState()
        ctx.addPath(circle)
        ctx.clip()

        // night sky, lighter towards the top
        let space = CGColorSpaceCreateDeviceRGB()
        let sky = CGGradient(colorsSpace: space, colors: [
            CGColor(red: 0.04, green: 0.08, blue: 0.17, alpha: 1),
            CGColor(red: 0.10, green: 0.20, blue: 0.38, alpha: 1),
        ] as CFArray, locations: [0, 1])!
        ctx.drawLinearGradient(sky, start: CGPoint(x: 0, y: disc.minY), end: CGPoint(x: 0, y: disc.maxY), options: [])

        // a few still flakes, the same every time
        var seed: UInt32 = 7
        func next() -> CGFloat { seed = seed &* 1_664_525 &+ 1_013_904_223; return CGFloat(seed >> 8) / CGFloat(1 << 24) }
        for _ in 0..<34 {
            let x = disc.minX + next() * disc.width, y = disc.minY + disc.height * (0.28 + 0.72 * next())
            let r = 0.8 + next() * 2.2
            // none over the text
            if abs(x - disc.midX) < 150 && y > disc.minY + 70 && y < disc.maxY - 50 { _ = next(); continue }
            ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.25 + 0.5 * next()))
            ctx.fillEllipse(in: CGRect(x: x - r, y: y - r, width: r * 2, height: r * 2))
        }

        // the drift along the bottom: two soft ridges
        ctx.setFillColor(CGColor(red: 0.78, green: 0.86, blue: 0.96, alpha: 1))
        let back = CGMutablePath()
        back.move(to: CGPoint(x: disc.minX, y: disc.minY))
        back.addLine(to: CGPoint(x: disc.minX, y: disc.minY + 52))
        back.addCurve(to: CGPoint(x: disc.midX + 10, y: disc.minY + 44), control1: CGPoint(x: disc.minX + 120, y: disc.minY + 86), control2: CGPoint(x: disc.midX - 90, y: disc.minY + 26))
        back.addCurve(to: CGPoint(x: disc.maxX, y: disc.minY + 58), control1: CGPoint(x: disc.midX + 110, y: disc.minY + 62), control2: CGPoint(x: disc.maxX - 70, y: disc.minY + 78))
        back.addLine(to: CGPoint(x: disc.maxX, y: disc.minY))
        back.closeSubpath()
        ctx.addPath(back)
        ctx.fillPath()
        ctx.setFillColor(CGColor(red: 0.96, green: 0.98, blue: 1, alpha: 1))
        let front = CGMutablePath()
        front.move(to: CGPoint(x: disc.minX, y: disc.minY))
        front.addLine(to: CGPoint(x: disc.minX, y: disc.minY + 30))
        front.addCurve(to: CGPoint(x: disc.midX - 20, y: disc.minY + 26), control1: CGPoint(x: disc.minX + 80, y: disc.minY + 50), control2: CGPoint(x: disc.midX - 110, y: disc.minY + 12))
        front.addCurve(to: CGPoint(x: disc.maxX, y: disc.minY + 36), control1: CGPoint(x: disc.midX + 60, y: disc.minY + 42), control2: CGPoint(x: disc.maxX - 90, y: disc.minY + 52))
        front.addLine(to: CGPoint(x: disc.maxX, y: disc.minY))
        front.closeSubpath()
        ctx.addPath(front)
        ctx.fillPath()
        ctx.restoreGState()

        ctx.addPath(circle)
        ctx.setStrokeColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.18))
        ctx.setLineWidth(1.5)
        ctx.strokePath()
    }

    private static func paragraph(lineHeight: CGFloat) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.lineHeightMultiple = lineHeight
        return style
    }
}

/// A six-armed snowflake with branches, drawn in code.
private final class SnowflakeView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let c = CGPoint(x: bounds.midX, y: bounds.midY)
        let R = min(bounds.width, bounds.height) / 2 - 3
        ctx.setLineCap(.round)
        for (width, color) in [(CGFloat(6.2), CGColor(red: 0.05, green: 0.12, blue: 0.26, alpha: 0.55)),
                               (CGFloat(2.6), CGColor(red: 0.86, green: 0.95, blue: 1, alpha: 1))] {
            ctx.setLineWidth(width)
            ctx.setStrokeColor(color)
            for k in 0..<6 {
                ctx.saveGState()
                ctx.translateBy(x: c.x, y: c.y)
                ctx.rotate(by: CGFloat(k) * .pi / 3)
                ctx.move(to: .zero)
                ctx.addLine(to: CGPoint(x: 0, y: R))
                for (at, len) in [(0.45, 0.28), (0.72, 0.2)] {
                    ctx.move(to: CGPoint(x: 0, y: R * at))
                    ctx.addLine(to: CGPoint(x: -R * len, y: R * (at + len)))
                    ctx.move(to: CGPoint(x: 0, y: R * at))
                    ctx.addLine(to: CGPoint(x: R * len, y: R * (at + len)))
                }
                ctx.strokePath()
                ctx.restoreGState()
            }
        }
    }
}

private final class GetStartedButton: NSButton {
    private let fill = NSColor(srgbRed: 0.96, green: 0.98, blue: 1, alpha: 1)
    private let label = NSColor(srgbRed: 0.06, green: 0.13, blue: 0.27, alpha: 1)

    init() {
        super.init(frame: .zero)
        title = tr("welcome.start")
        isBordered = false
        keyEquivalent = "\r"
        focusRingType = .none
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([widthAnchor.constraint(equalToConstant: 168), heightAnchor.constraint(equalToConstant: 40)])
        setAccessibilityLabel(tr("welcome.start"))
    }

    required init?(coder: NSCoder) { nil }

    override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(roundedRect: bounds, xRadius: bounds.height / 2, yRadius: bounds.height / 2)
        (isHighlighted ? fill.blended(withFraction: 0.14, of: .black) ?? fill : fill).setFill()
        shape.fill()
        let text = NSAttributedString(string: tr("welcome.start"), attributes: [
            .font: NSFont.systemFont(ofSize: 14, weight: .semibold),
            .foregroundColor: label,
        ])
        let size = text.size()
        text.draw(at: CGPoint(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
    }
}
