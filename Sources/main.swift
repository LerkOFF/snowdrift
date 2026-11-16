import Cocoa

// Snowdrift: снегопад поверх всех окон macOS. Клики проходят сквозь, иконки в Dock нет,
// управление из меню-бара.

func tr(_ ru: String, _ en: String) -> String {
    (Locale.preferredLanguages.first ?? "en").hasPrefix("ru") ? ru : en
}

let appVersion = (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "dev"
if CommandLine.arguments.contains("--version") {
    print("Snowdrift \(appVersion)")
    exit(0)
}

// Снежинки рисуются вручную: у каждой своя глубина, скорость падения и покачивание.

func makeFlakeImage(diameter: Int) -> CGImage? {
    let space = CGColorSpaceCreateDeviceRGB()
    guard let ctx = CGContext(data: nil, width: diameter, height: diameter, bitsPerComponent: 8, bytesPerRow: 0,
                              space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
    let colors = [CGColor(red: 1, green: 1, blue: 1, alpha: 1),
                  CGColor(red: 1, green: 1, blue: 1, alpha: 0.9),
                  CGColor(red: 1, green: 1, blue: 1, alpha: 0)] as CFArray
    let grad = CGGradient(colorsSpace: space, colors: colors, locations: [0, 0.5, 1])!
    let c = CGPoint(x: CGFloat(diameter) / 2, y: CGFloat(diameter) / 2)
    ctx.drawRadialGradient(grad, startCenter: c, startRadius: 0, endCenter: c, endRadius: CGFloat(diameter) / 2, options: [])
    return ctx.makeImage()
}

struct Flake {
    var x: CGFloat
    var y: CGFloat          // от верха экрана вниз
    var depth: CGFloat      // 0 далеко ... 1 близко
    var phase: CGFloat
    var swayFreq: CGFloat
    var swayAmp: CGFloat
}

/// Ком снега, оторвавшийся от сугроба: падает под действием гравитации.
struct Clump {
    var x: CGFloat
    var y: CGFloat
    var vx: CGFloat
    var vy: CGFloat
    var vol: CGFloat        // объём: высота колонки * ширина колонки
    var skipId: Int         // окно, с которого сорвался (первое время на него не садится)
    var age: CGFloat
}

/// Сугроб на верхней кромке окна. Колонки привязаны к ЭКРАНУ (индекс = x / colW), а не к окну:
/// снег лежит там, где лёг. Нет опоры под колонкой - она падает.
/// Снегоуборочная машина: едет по нижнему краю, сгребает снег и сбрасывает его за край экрана.
struct Plow {
    var dir: CGFloat        // +1 вправо, -1 влево
    var tip: CGFloat        // x кончика ковша
    var pushed: CGFloat     // сколько снега сгребла (для размера вала перед ковшом)
    var dumped: Bool
}

final class Ledge {
    static let colW: CGFloat = 4
    let id: Int
    var rect: CGRect        // в координатах вида, ось Y сверху вниз
    let cap: CGFloat
    let isFloor: Bool
    var h: [CGFloat]        // высота колонки
    var base: [CGFloat]     // y кромки, на которой лежит колонка
    var exposed: [Bool]
    var vel = CGPoint.zero
    var accel: CGFloat = 0
    var prevTop: CGFloat    // y кромки в прошлом кадре: нужен, чтобы окно ловило снег, двигаясь навстречу
    init(id: Int, rect: CGRect, cols: Int, cap: CGFloat = 16, isFloor: Bool = false) {
        self.id = id
        self.rect = rect
        self.prevTop = rect.minY
        self.cap = cap
        self.isFloor = isFloor
        h = Array(repeating: 0, count: cols)
        base = Array(repeating: rect.minY, count: cols)
        exposed = Array(repeating: true, count: cols)
    }
}

final class SnowView: NSView {
    var flakes: [Flake] = []
    var clumps: [Clump] = []
    var last = CACurrentMediaTime()
    let startTime = CACurrentMediaTime()
    var timer: Timer?
    let image = makeFlakeImage(diameter: 64)
    let screenFrame: CGRect
    let primaryHeight: CGFloat
    let cols: Int
    var accumulate = true { didSet { if !accumulate { clearSnow() } } }
    var ledges: [Int: Ledge] = [:]
    var ordered: [Ledge] = []      // спереди назад, в конце пол
    let groundLedge: Ledge         // сугроб по самому низу экрана
    var frameCounter = 0
    let rate: CGFloat
    let demo: Bool
    var plow: Plow?
    let plowTrigger: CGFloat = 40     // средняя высота снега внизу, при которой выезжает уборщик

    init(frame: NSRect, screenFrame: CGRect, primaryHeight: CGFloat, intensity: Float) {
        self.screenFrame = screenFrame
        self.primaryHeight = primaryHeight
        let env = ProcessInfo.processInfo.environment
        if let r = env["SNOW_RATE"], let v = Double(r) { rate = CGFloat(v) } else { rate = 1 }
        demo = env["SNOW_DEMO"] != nil
        cols = Int(ceil(frame.width / Ledge.colW)) + 1
        groundLedge = Ledge(id: -1, rect: CGRect(x: 0, y: frame.height, width: frame.width, height: 1),
                      cols: Int(ceil(frame.width / Ledge.colW)) + 1, cap: 48, isFloor: true)
        super.init(frame: frame)
        let count = Int(Float(frame.width * frame.height) / (1440 * 900) * 220 * intensity)
        for _ in 0..<max(count, 20) {
            let d = CGFloat.random(in: 0...1)
            flakes.append(Flake(x: .random(in: 0...frame.width), y: .random(in: 0...frame.height),
                                depth: d * d, phase: .random(in: 0...(2 * .pi)),
                                swayFreq: .random(in: 0.3...0.9), swayAmp: .random(in: 8...28)))
        }
        if env["SNOW_PLOWTEST"] != nil { for c in groundLedge.h.indices { groundLedge.h[c] = 44 } }
        let t = Timer(timeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }
    required init?(coder: NSCoder) { fatalError() }
    deinit { timer?.invalidate() }

    override var isOpaque: Bool { false }

    func clearSnow() {
        ledges = [:]; ordered = []; clumps = []; plow = nil
        for c in groundLedge.h.indices { groundLedge.h[c] = 0 }
    }

    // MARK: окна

    typealias Item = (id: Int, rect: CGRect, ok: Bool)

    func demoItems(t: Double) -> [Item] {
        var x: CGFloat = 300, y: CGFloat = 320
        if t > 14 && t < 14.5 { y -= 170 * sin(CGFloat(t - 14) / 0.5 * .pi) }   // подбросили
        if t > 20 { x += 260 * min(1, CGFloat((t - 20) / 1.2)) }
        return [(1000, CGRect(x: x, y: y, width: 600, height: 260), true)]
    }

    func collectItems() -> [Item] {
        if demo { return demoItems(t: CACurrentMediaTime() - startTime) }
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return [] }
        let myPid = Int32(ProcessInfo.processInfo.processIdentifier)
        let topOffset = primaryHeight - screenFrame.maxY
        var items: [Item] = []
        for w in list {
            guard (w[kCGWindowLayer as String] as? Int) == 0,
                  let pid = w[kCGWindowOwnerPID as String] as? Int32, pid != myPid,
                  let num = w[kCGWindowNumber as String] as? Int,
                  let bd = w[kCGWindowBounds as String] as? NSDictionary,
                  var r = CGRect(dictionaryRepresentation: bd as CFDictionary) else { continue }
            let alpha = (w[kCGWindowAlpha as String] as? Double) ?? 1
            if alpha < 0.1 || r.width < 2 || r.height < 2 { continue }
            r.origin.x -= screenFrame.minX
            r.origin.y -= topOffset
            let ok = r.width >= 160 && r.height >= 90 && r.minY >= 40 && r.minY <= bounds.height - 20
                && r.maxX > 20 && r.minX < bounds.width - 20 && alpha > 0.5
            items.append((num, r, ok))
        }
        return items
    }

    func refreshWindows(dt: CGFloat) {
        let items = collectItems()
        var newLedges: [Int: Ledge] = [:]
        var order: [Ledge] = []
        for (k, it) in items.enumerated() where it.ok {
            let led: Ledge
            if let old = ledges[it.id] {
                led = old
                led.prevTop = old.rect.minY
                let d = max(dt, 1.0 / 240)
                let inst = CGPoint(x: (it.rect.minX - old.rect.minX) / d, y: (it.rect.minY - old.rect.minY) / d)
                let nv = CGPoint(x: old.vel.x * 0.5 + inst.x * 0.5, y: old.vel.y * 0.5 + inst.y * 0.5)
                let a = hypot(nv.x - old.vel.x, nv.y - old.vel.y) / d
                led.accel = led.accel * 0.6 + a * 0.4
                led.vel = nv
                led.rect = it.rect
            } else {
                led = Ledge(id: it.id, rect: it.rect, cols: cols)
            }
            // видимость кромки: закрыта ли она окнами, лежащими выше
            let top = it.rect.minY
            let c0 = max(0, Int(floor(it.rect.minX / Ledge.colW)))
            let c1 = min(cols - 1, Int(floor(it.rect.maxX / Ledge.colW)))
            if c0 <= c1 {
                for c in c0...c1 {
                    let x = (CGFloat(c) + 0.5) * Ledge.colW
                    var hidden = false
                    for j in 0..<k {
                        let r = items[j].rect
                        if r.contains(CGPoint(x: x, y: top - 2)) || r.contains(CGPoint(x: x, y: top + 2)) { hidden = true; break }
                    }
                    led.exposed[c] = !hidden
                }
            }
            supportStep(led)
            newLedges[it.id] = led
            order.append(led)
        }
        // окно пропало (закрыто, уехало с экрана, свернуто): весь снег с него осыпается
        for (id, old) in ledges where newLedges[id] == nil { shedAll(old) }
        ledges = newLedges
        order.append(groundLedge)
        ordered = order
    }

    /// Колонка без опоры (окно уехало из-под неё или опустилось) отрывается и падает.
    /// При резком рывке окна часть снега стряхивается.
    func supportStep(_ led: Ledge) {
        let T = led.rect.minY
        for c in 0..<cols where led.h[c] > 0 {
            let x = (CGFloat(c) + 0.5) * Ledge.colW
            if x < led.rect.minX || x > led.rect.maxX || T - led.base[c] > 4 {
                shed(led, c, fraction: 1)
            } else {
                led.base[c] = T       // окно поднялось: подтолкнуло снег вверх
            }
        }
        if led.accel > 4000 {
            let frac = min(0.6, (led.accel - 4000) / 20000)
            for c in 0..<cols where led.h[c] > 0.3 {
                if CGFloat.random(in: 0...1) < min(1, frac * 8) { shed(led, c, fraction: 0.7, kick: true) }
            }
        }
    }

    func shed(_ led: Ledge, _ c: Int, fraction: CGFloat, kick: Bool = false) {
        let old = led.h[c]
        let dh = fraction >= 1 ? old : old * fraction
        led.h[c] = fraction >= 1 ? 0 : old - dh
        if dh < 0.08 || clumps.count > 1500 { return }
        clumps.append(Clump(x: (CGFloat(c) + 0.5) * Ledge.colW + CGFloat.random(in: -1.5...1.5),
                            y: led.base[c] - old / 2,
                            vx: led.vel.x * 0.85 + CGFloat.random(in: -20...20),
                            vy: led.vel.y * 0.85 - (kick ? CGFloat.random(in: 0...50) : 0),
                            vol: dh * Ledge.colW, skipId: led.id, age: 0))
    }

    func shedAll(_ led: Ledge) {
        for c in 0..<cols where led.h[c] > 0 { shed(led, c, fraction: 1) }
    }

    func land(on led: Ledge, at x: CGFloat, amount: CGFloat) {
        let c = Int(x / Ledge.colW)
        let weights: [CGFloat] = [0.1, 0.2, 0.4, 0.2, 0.1]
        for (i, wgt) in weights.enumerated() {
            let ci = c + i - 2
            guard ci >= 0 && ci < cols else { continue }
            let cx = (CGFloat(ci) + 0.5) * Ledge.colW
            if cx < led.rect.minX || cx > led.rect.maxX { continue }
            if led.h[ci] < 0.05 { led.base[ci] = led.rect.minY }
            led.h[ci] = min(led.cap, led.h[ci] + amount * wgt)
        }
    }

    func relax(_ l: Ledge) {
        let c0 = max(1, Int(ceil(l.rect.minX / Ledge.colW)) + 1)
        let c1 = min(cols - 2, Int(floor(l.rect.maxX / Ledge.colW)) - 1)
        if c1 <= c0 { return }
        var out = l.h
        for i in c0...c1 { out[i] = l.h[i] + 0.15 * (l.h[i - 1] + l.h[i + 1] - 2 * l.h[i]) }
        l.h = out
    }

    // MARK: снегоуборщик

    func startPlow() {
        guard plow == nil, accumulate else { return }
        let d: CGFloat = Bool.random() ? 1 : -1
        plow = Plow(dir: d, tip: d > 0 ? 0 : bounds.width, pushed: 0, dumped: false)
    }

    func updatePlow(dt: CGFloat) {
        let w = bounds.width, h = bounds.height
        if plow == nil && frameCounter % 30 == 0 && accumulate {
            let mean = groundLedge.h.reduce(0, +) / CGFloat(cols)
            if mean >= plowTrigger { startPlow() }
        }
        guard var p = plow else { return }
        let prev = p.tip
        p.tip += p.dir * 230 * dt
        let a = max(0, min(prev, p.tip)), b = min(w, max(prev, p.tip))
        if b >= a {
            let c0 = max(0, Int(a / Ledge.colW)), c1 = min(cols - 1, Int(b / Ledge.colW))
            for c in c0...c1 { p.pushed += groundLedge.h[c] * Ledge.colW; groundLedge.h[c] = 0 }
        }
        let atEdge = p.dir > 0 ? p.tip >= w : p.tip <= 0
        if atEdge && !p.dumped {
            // вал снега перевалился за край экрана и исчез
            for _ in 0..<40 {
                clumps.append(Clump(x: p.dir > 0 ? w - 4 : 4, y: h - CGFloat.random(in: 0...40),
                                    vx: p.dir * CGFloat.random(in: 120...260), vy: -CGFloat.random(in: 40...220),
                                    vol: CGFloat.random(in: 8...30), skipId: -99, age: 0))
            }
            p.dumped = true
            p.pushed = 0
        }
        if (p.dir > 0 && p.tip - 150 > w) || (p.dir < 0 && p.tip + 150 < 0) { plow = nil } else { plow = p }
    }

    func drawPlow(_ ctx: CGContext, _ p: Plow) {
        let moundH = 12 + 58 * (1 - exp(-p.pushed / 15000))
        let moundW = 50 + moundH * 0.8
        if !p.dumped && p.pushed > 0 {
            let f = p.dir
            let endX = p.tip + f * moundW
            let endH = min(moundH, 24)
            ctx.beginPath()
            ctx.move(to: CGPoint(x: p.tip, y: 0))
            ctx.addLine(to: CGPoint(x: p.tip, y: moundH))
            ctx.addCurve(to: CGPoint(x: endX, y: endH), control1: CGPoint(x: p.tip + f * moundW * 0.35, y: moundH * 1.02),
                         control2: CGPoint(x: p.tip + f * moundW * 0.7, y: moundH * 0.5))
            ctx.addLine(to: CGPoint(x: endX, y: 0))
            ctx.closePath()
            ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.97))
            ctx.fillPath()
        }
        ctx.saveGState()
        ctx.translateBy(x: p.tip, y: 0)
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
        if p.pushed > 0 && !p.dumped {
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

    // MARK: Dock

    var dockH: CGFloat = 0
    var dockAtBottom = true

    func refreshDock() {
        guard let s = NSScreen.screens.first(where: { $0.frame == screenFrame }) else { return }
        dockAtBottom = s.visibleFrame.minX <= s.frame.minX + 1 && s.visibleFrame.maxX >= s.frame.maxX - 1
        dockH = max(0, s.visibleFrame.minY - s.frame.minY)
    }

    /// Курсор над Dock: иконка под ним вырастает, и снег рядом разлетается в стороны и вверх.
    func dockHover(dt: CGFloat) {
        if frameCounter % 60 == 1 { refreshDock() }
        guard dockAtBottom else { return }
        let m = NSEvent.mouseLocation
        guard screenFrame.contains(m) else { return }
        let zone = dockH > 0 ? dockH + 10 : 70     // Dock скрыт автоматически: полоса у нижнего края
        guard m.y - screenFrame.minY <= zone else { return }
        let mx = m.x - screenFrame.minX
        let reach: CGFloat = 95                     // примерно ширина увеличенной иконки с соседями
        let c0 = max(0, Int((mx - reach) / Ledge.colW)), c1 = min(cols - 1, Int((mx + reach) / Ledge.colW))
        guard c0 <= c1 else { return }
        for c in c0...c1 {
            let x = (CGFloat(c) + 0.5) * Ledge.colW
            let d = x - mx
            let wgt = 1 - abs(d) / reach
            let old = groundLedge.h[c]
            guard wgt > 0, old > 0.3 else { continue }
            if CGFloat.random(in: 0...1) > min(1, 14 * dt * wgt) { continue }
            let take = old * CGFloat.random(in: 0.5...1)
            groundLedge.h[c] = old - take
            if clumps.count > 1500 { continue }
            let side: CGFloat = d >= 0 ? 1 : -1
            clumps.append(Clump(x: x, y: groundLedge.base[c] - old / 2,
                                vx: side * CGFloat.random(in: 40...(60 + 170 * wgt)),
                                vy: -CGFloat.random(in: 120...(220 + 280 * wgt)),
                                vol: take * Ledge.colW, skipId: groundLedge.id, age: 0))
        }
    }

    // MARK: кадр

    func tick() {
        let now = CACurrentMediaTime()
        let dt = CGFloat(min(now - last, 0.1))
        last = now
        let w = bounds.width, h = bounds.height
        frameCounter += 1
        if accumulate { refreshWindows(dt: dt) }
        if accumulate { updatePlow(dt: dt); dockHover(dt: dt) }
        for i in flakes.indices {
            let oldY = flakes[i].y
            let speed = 22 + flakes[i].depth * 90
            flakes[i].y += speed * dt
            flakes[i].phase += flakes[i].swayFreq * dt
            flakes[i].x += cos(flakes[i].phase) * flakes[i].swayAmp * flakes[i].swayFreq * dt * (0.5 + flakes[i].depth)
            if flakes[i].y > h + 20 { flakes[i].y = -20; flakes[i].x = .random(in: 0...w) }
            if flakes[i].x > w + 20 { flakes[i].x = -20 }
            if flakes[i].x < -20 { flakes[i].x = w + 20 }
            if accumulate {
                let x = flakes[i].x, y = flakes[i].y
                for led in ordered {
                    let top = led.rect.minY
                    if oldY < led.prevTop && y >= top && x > led.rect.minX + 6 && x < led.rect.maxX - 6 {
                        let c = Int(x / Ledge.colW)
                        if c >= 0 && c < led.exposed.count && led.exposed[c] && CGFloat.random(in: 0...1) < 0.55 {
                            land(on: led, at: x, amount: (0.4 + flakes[i].depth) * 12 * rate)
                            flakes[i].y = -20
                            flakes[i].x = .random(in: 0...w)
                        }
                        break
                    }
                }
            }
        }
        // комья снега: гравитация и посадка на ближайшую опору
        var i = clumps.count - 1
        while i >= 0 {
            var cl = clumps[i]
            let oldY = cl.y
            cl.vy = min(cl.vy + 900 * dt, 700)
            cl.vx *= max(0, 1 - 1.5 * dt)
            cl.y += cl.vy * dt
            cl.x += cl.vx * dt
            cl.age += dt
            var done = cl.y > h + 30 || cl.x < -30 || cl.x > w + 30
            if !done && accumulate {
                for led in ordered {
                    if led.id == cl.skipId && cl.age < 0.15 { continue }
                    let top = led.rect.minY
                    if oldY < led.prevTop && cl.y >= top && cl.x >= led.rect.minX && cl.x <= led.rect.maxX {
                        let c = Int(cl.x / Ledge.colW)
                        if c >= 0 && c < led.exposed.count && led.exposed[c] {
                            land(on: led, at: cl.x, amount: cl.vol / Ledge.colW)
                            done = true
                            break
                        }
                    }
                }
            }
            if done { clumps.remove(at: i) } else { clumps[i] = cl }
            i -= 1
        }
        if accumulate && frameCounter % 4 == 0 { ordered.forEach(relax) }
        needsDisplay = true
    }

    func drawLedge(_ ctx: CGContext, _ l: Ledge) {
        let n = l.h.count
        let H = bounds.height
        var c = 0
        while c < n {
            if !l.exposed[c] || l.h[c] < 0.25 { c += 1; continue }
            var e = c
            while e < n && l.exposed[e] && l.h[e] >= 0.25 { e += 1 }
            var topPts: [CGPoint] = []
            var botPts: [CGPoint] = []
            for i in c..<e {
                let a = i > 0 ? l.h[i - 1] : l.h[i]
                let b = i < n - 1 ? l.h[i + 1] : l.h[i]
                let sm = (a + 2 * l.h[i] + b) / 4
                let x = (CGFloat(i) + 0.5) * Ledge.colW
                let dist = max(0, min(x - l.rect.minX, l.rect.maxX - x))
                let taper = min(1, dist / 14)
                let by = H - l.base[i] - 1
                topPts.append(CGPoint(x: x, y: by + sm * taper + 1))
                botPts.append(CGPoint(x: x, y: by))
            }
            guard let f = botPts.first, let la = botPts.last else { c = e; continue }
            var pts: [CGPoint] = [CGPoint(x: CGFloat(c) * Ledge.colW, y: f.y)]
            pts += topPts
            pts.append(CGPoint(x: CGFloat(e) * Ledge.colW, y: la.y))
            pts += botPts.reversed()
            ctx.addLines(between: pts)
            ctx.closePath()
            ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.96))
            ctx.fillPath()
            c = e
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext, let img = image else { return }
        ctx.setBlendMode(.clear)
        ctx.fill(bounds)
        ctx.setBlendMode(.normal)
        let H = bounds.height
        if demo, let it = demoItems(t: CACurrentMediaTime() - startTime).first {
            ctx.setFillColor(CGColor(red: 0.18, green: 0.18, blue: 0.2, alpha: 1))
            ctx.fill(CGRect(x: it.rect.minX, y: H - it.rect.maxY, width: it.rect.width, height: it.rect.height))
        }
        if accumulate { ordered.forEach { drawLedge(ctx, $0) } }
        if let p = plow { drawPlow(ctx, p) }
        for cl in clumps {
            let size = max(3, min(10, sqrt(cl.vol) * 1.1))
            ctx.setAlpha(0.95)
            ctx.draw(img, in: CGRect(x: cl.x - size / 2, y: H - cl.y - size / 2, width: size, height: size))
        }
        for f in flakes {
            let size = 4 + f.depth * 12
            ctx.setAlpha(0.35 + f.depth * 0.65)
            ctx.draw(img, in: CGRect(x: f.x - size / 2, y: H - f.y - size / 2, width: size, height: size))
        }
    }
}

final class SnowWindow: NSWindow {
    init(screen: NSScreen, intensity: Float) {
        super.init(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        level = .screenSaver
        collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary, .ignoresCycle]
        isReleasedWhenClosed = false
        contentView = SnowView(frame: CGRect(origin: .zero, size: screen.frame.size), screenFrame: screen.frame,
                               primaryHeight: NSScreen.screens.first?.frame.height ?? screen.frame.height, intensity: intensity)
        setFrame(screen.frame, display: true)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var windows: [SnowWindow] = []
    var statusItem: NSStatusItem!
    var intensity: Float = 1.0
    var paused = false
    var accumulate = true

    func applicationDidFinishLaunching(_ n: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.title = "❄️"
        rebuildMenu()
        rebuildWindows()
        NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification,
                                               object: nil, queue: .main) { [weak self] _ in self?.rebuildWindows() }
    }

    func rebuildWindows() {
        windows.forEach { ($0.contentView as? SnowView)?.timer?.invalidate(); $0.orderOut(nil) }
        windows = []
        guard !paused else { return }
        for s in NSScreen.screens {
            let w = SnowWindow(screen: s, intensity: intensity)
            (w.contentView as? SnowView)?.accumulate = accumulate
            w.orderFrontRegardless()
            windows.append(w)
        }
    }

    func rebuildMenu() {
        let m = NSMenu()
        let toggle = NSMenuItem(title: paused ? tr("Включить снег", "Resume snow") : tr("Пауза", "Pause"), action: #selector(togglePause), keyEquivalent: "")
        toggle.target = self
        m.addItem(toggle)
        m.addItem(.separator())
        for (title, value) in [(tr("Лёгкий снег", "Light snow"), Float(0.5)), (tr("Обычный", "Normal"), 1.0), (tr("Метель", "Blizzard"), 2.5)] {
            let it = NSMenuItem(title: title, action: #selector(setIntensity(_:)), keyEquivalent: "")
            it.target = self
            it.representedObject = value
            it.state = intensity == value ? .on : .off
            m.addItem(it)
        }
        m.addItem(.separator())
        let acc = NSMenuItem(title: tr("Копить снег на окнах", "Pile snow on windows"), action: #selector(toggleAccumulate), keyEquivalent: "")
        acc.target = self
        acc.state = accumulate ? .on : .off
        m.addItem(acc)
        let shake = NSMenuItem(title: tr("Стряхнуть сугробы", "Shake off the snow"), action: #selector(shakeSnow), keyEquivalent: "")
        shake.target = self
        m.addItem(shake)
        let plowItem = NSMenuItem(title: tr("Вызвать снегоуборщик", "Call the snowplow"), action: #selector(callPlow), keyEquivalent: "")
        plowItem.target = self
        m.addItem(plowItem)
        m.addItem(.separator())
        m.addItem(NSMenuItem(title: tr("Выход", "Quit"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        statusItem.menu = m
    }

    @objc func toggleAccumulate() {
        accumulate.toggle()
        windows.forEach { ($0.contentView as? SnowView)?.accumulate = accumulate }
        rebuildMenu()
    }
    @objc func callPlow() { windows.forEach { ($0.contentView as? SnowView)?.startPlow() } }
    @objc func shakeSnow() { windows.forEach { ($0.contentView as? SnowView)?.clearSnow() } }
    @objc func togglePause() { paused.toggle(); rebuildMenu(); rebuildWindows() }
    @objc func setIntensity(_ s: NSMenuItem) {
        intensity = (s.representedObject as? Float) ?? 1
        paused = false
        rebuildMenu(); rebuildWindows()
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let delegate = AppDelegate()
app.delegate = delegate
app.run()
