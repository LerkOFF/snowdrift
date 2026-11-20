import Foundation
import simd

// The winter decorations: a string of lights along the top of each window, snowmen the user rolls and stacks out of
// the snow on the ground, and frost creeping in from the edges of the screen that the pointer wipes clear.

// MARK: - Garland

/// A string of lights hung along the top edge of a window.
struct GarlandSprite {
    /// The left end of the window's top edge, in overlay coordinates.
    var anchor: SIMD2<Float>
    var width: Float
    var seed: Float
    /// Frames of windows in front of it.
    var covers: [SIMD4<Float>]
}

// MARK: - Snowmen

/// One ball of snow as the renderer draws it.
struct BallSprite {
    var center: SIMD2<Float>
    var radius: Float
    var seed: Float
    var roll: Float
    /// 0 a plain ball, 1 the body of a snowman (buttons and arms), 2 its head (eyes, nose, mouth and scarf).
    var role: Float
    /// 1 just as the snowman was finished, easing to 0: the face pops in.
    var pop: Float
}

/// What the snowmen need of the snow under them, handed over by the simulation.
struct SnowTerrain {
    struct Probe {
        /// Where a ball rests at this x.
        var surface: Float
        /// How deep the snow is there.
        var depth: Float
        /// How far under the surface the pointer still counts as on the snow: the ground reaches down to the edge of
        /// the screen, the Dock's top is a thin line over its icons.
        var below: Float
    }

    var width: Float
    var probe: (Float) -> Probe
    /// Takes `share` of the snow from the columns within `half` of x, and returns the area taken.
    var take: (Float, Float, Float) -> Float
    /// Throws an area of snow into the air at a point.
    var burst: (Float, SIMD2<Float>, SIMD2<Float>) -> Void
}

/// Balls of snow rolled up out of the snow on the ground, carried about and stacked into snowmen.
final class Snowmen {
    struct Ball {
        let id: Int
        var pos: SIMD2<Float>
        var vel = SIMD2<Float>(repeating: 0)
        var mass: Float
        /// The ball it rests on, once stacked.
        var support: Int?
        let seed: Float
        var roll: Float = 0
        var settled: Float = 0
        var airborne = false
        /// Burst or off the screen: taken out of the list once the step is over.
        var gone = false
        /// When it got its face, if it has one.
        var adorned: Double?

        var radius: Float { Snowmen.radius(of: mass) }
    }

    private enum Target: Equatable {
        case none
        case ball(Int)
        case snow
    }

    static let density: Float = 1.25
    static let smallest: Float = 6
    static let largest: Float = 54
    private static let gravity: Float = 2400
    /// A ball thrown or dropped harder than this bursts.
    private static let breaks: Float = 1250

    static func radius(of mass: Float) -> Float { (max(mass, 0) / density).squareRoot() }
    static func mass(of radius: Float) -> Float { radius * radius * density }

    var enabled = false {
        didSet {
            guard enabled != oldValue else { return }
            if !enabled { clear() }
        }
    }

    private(set) var balls: [Ball] = []
    /// The pointer is over something to take hold of: the overlay catches its clicks instead of letting them through.
    private(set) var wantsPointer = false
    /// The pointer is holding a ball or rolling one: the snow it passes over is for the ball, not to be swept.
    private(set) var holding = false
    private var nextID = 1
    private var heldID: Int?
    private var grab = SIMD2<Float>(repeating: 0)
    private var wasDown = false
    private var previous: SIMD2<Float>?
    private var velocity = SIMD2<Float>(repeating: 0)
    private var hover = Target.none
    /// When the last ball moved: the scene is drawn at full rate meanwhile.
    private(set) var movedAt: Double = -10

    var isEmpty: Bool { balls.isEmpty }

    func clear() {
        balls.removeAll()
        heldID = nil
        hover = .none
        wantsPointer = false
        holding = false
    }

    /// Bursts every ball: what the snow plough does to those in its way, or a click on the menu.
    func smashAll(_ terrain: SnowTerrain) {
        for ball in balls { terrain.burst(ball.mass * 0.9, ball.pos, SIMD2(0, 120)) }
        clear()
    }

    /// Bursts the balls the blade of the snow plough passes over between two x positions.
    func smash(from: Float, to: Float, _ terrain: SnowTerrain) {
        let low = min(from, to), high = max(from, to)
        var gone = Set<Int>()
        for ball in balls where ball.pos.x + ball.radius >= low && ball.pos.x - ball.radius <= high && ball.id != heldID {
            gone.insert(ball.id)
        }
        guard !gone.isEmpty else { return }
        for ball in balls where gone.contains(ball.id) || (ball.support.map(gone.contains) ?? false) {
            gone.insert(ball.id)
            terrain.burst(ball.mass * 0.9, ball.pos, SIMD2(random(ball.seed) * 160, 150))
        }
        balls.removeAll { gone.contains($0.id) }
    }

    private func random(_ seed: Float) -> Float {
        let v = sin(seed * 127.1) * 43758.5453
        return (v - v.rounded(.down)) * 2 - 1
    }

    /// Advances the balls, and the user's hold on them. `pointer` is nil while the pointer is out of reach.
    func update(dt: Float, pointer: SIMD2<Float>?, down: Bool, terrain: SnowTerrain, clock: Double) -> Bool {
        guard enabled || !balls.isEmpty else {
            wantsPointer = false
            holding = false
            return false
        }
        var changed = false
        if let pointer {
            if let previous, dt > 0 {
                let raw = (pointer - previous) / max(dt, 1.0 / 240)
                velocity += (raw - velocity) * min(1, dt * 14)
            }
            previous = pointer
        } else {
            previous = nil
            velocity = .zero
        }

        let pressed = down && !wasDown
        wasDown = down
        if heldID == nil {
            hover = enabled && pointer != nil ? target(at: pointer!, terrain) : .none
            if pressed, let pointer {
                switch hover {
                case .ball(let id): pickUp(id, at: pointer)
                case .snow: roll(at: pointer, terrain)
                case .none: break
                }
            }
        } else if !down || pointer == nil {
            drop()
        }
        wantsPointer = heldID != nil || hover != .none
        holding = heldID != nil

        if let id = heldID, let i = balls.firstIndex(where: { $0.id == id }), let pointer {
            moveHeld(i, to: pointer + grab, dt: dt, terrain: terrain)
            changed = true
        }
        for i in balls.indices where balls[i].id != heldID {
            if advance(i, dt: dt, terrain: terrain) { changed = true }
        }
        if balls.contains(where: \.gone) {
            let lost = Set(balls.filter(\.gone).map(\.id))
            balls.removeAll(where: \.gone)
            for i in balls.indices where balls[i].support.map(lost.contains) ?? false {
                balls[i].support = nil
                balls[i].airborne = true
            }
            changed = true
        }
        adorn(clock: clock)
        if changed || heldID != nil { movedAt = clock }
        return changed || heldID != nil
    }

    // MARK: Taking hold

    private func target(at point: SIMD2<Float>, _ terrain: SnowTerrain) -> Target {
        // The topmost ball first; one with another on top of it is out of reach.
        for ball in balls.sorted(by: { $0.pos.y > $1.pos.y }) where simd_distance(point, ball.pos) <= ball.radius + 3 {
            return balls.contains { $0.support == ball.id } ? .none : .ball(ball.id)
        }
        let probe = terrain.probe(point.x)
        if probe.depth >= 1.5 && point.y >= probe.surface - probe.below && point.y <= probe.surface + 12 { return .snow }
        return .none
    }

    private func pickUp(_ id: Int, at point: SIMD2<Float>) {
        guard let i = balls.firstIndex(where: { $0.id == id }) else { return }
        heldID = id
        grab = balls[i].pos - point
        balls[i].support = nil
        balls[i].airborne = false
        balls[i].vel = .zero
        balls[i].adorned = nil
        balls[i].settled = 0
    }

    /// Starts a ball in the snow at the pointer: dragged along the ground it picks up the snow it rolls over.
    private func roll(at point: SIMD2<Float>, _ terrain: SnowTerrain) {
        let ball = Ball(id: nextID, pos: point, mass: Self.mass(of: Self.smallest), support: nil, seed: Float.random(in: 0.01...1))
        nextID += 1
        balls.append(ball)
        heldID = ball.id
        grab = .zero
        // Several balls in one place would be a heap: the count is kept to what fits a screen.
        if balls.count > 14 { balls.removeFirst() }
    }

    private func drop() {
        guard let id = heldID, let i = balls.firstIndex(where: { $0.id == id }) else {
            heldID = nil
            return
        }
        heldID = nil
        var v = velocity
        let speed = simd_length(v)
        if speed > 2800 { v *= 2800 / speed }
        balls[i].vel = speed > 220 ? v * 0.8 : v * 0.2
        balls[i].airborne = true
        balls[i].settled = 0
    }

    private func moveHeld(_ i: Int, to target: SIMD2<Float>, dt: Float, terrain: SnowTerrain) {
        var ball = balls[i]
        let r = ball.radius
        let from = ball.pos.x
        let ease = min(1, dt * 30)
        ball.pos.x += (target.x - ball.pos.x) * ease
        ball.pos.x = min(max(ball.pos.x, 0), terrain.width)
        let floor = rest(ball.pos.x, r, y: ball.pos.y, ignoring: ball.id, terrain).y
        ball.pos.y = max(floor, ball.pos.y + (target.y - ball.pos.y) * ease)
        let travel = abs(ball.pos.x - from)
        if travel > 0.001 {
            ball.roll += (ball.pos.x - from) / max(r, 1)
            // On the snow and rolling: the snow it passes over wraps around it.
            if ball.pos.y - floor < 2 && r < Self.largest && terrain.probe(ball.pos.x).depth > 0.3 {
                let share = min(1, travel / max(r * 1.1, 1)) * 0.9
                let taken = terrain.take(ball.pos.x, r * 0.55, share)
                // Snow packs as it is rolled up: the ball gains less than the snow it took up.
                ball.mass = min(ball.mass + taken * 0.5, Self.mass(of: Self.largest))
            }
        }
        balls[i] = ball
    }

    // MARK: Resting and falling

    /// The balls resting on a ball, and those on them.
    private func above(_ id: Int) -> Set<Int> {
        var found = Set<Int>()
        var grew = true
        while grew {
            grew = false
            for ball in balls where !found.contains(ball.id) {
                if let support = ball.support, support == id || found.contains(support) {
                    found.insert(ball.id)
                    grew = true
                }
            }
        }
        return found
    }

    /// Where a ball of radius r at x comes to rest: on the snow or on the ball under it, and which ball that is. Only
    /// the balls lower than it hold it up (a held one rides over any), never those resting on it.
    private func rest(_ x: Float, _ r: Float, y: Float, ignoring id: Int, _ terrain: SnowTerrain) -> (y: Float, on: Ball?) {
        var y0 = terrain.probe(x).surface + r * 0.94
        var on: Ball?
        let held = id == heldID
        let over = above(id)
        for other in balls where other.id != id && other.id != heldID && !over.contains(other.id) {
            if !held && !(other.pos.y < y || (other.pos.y == y && other.id < id)) { continue }
            let dx = x - other.pos.x
            let reach = r + other.radius
            guard abs(dx) < reach else { continue }
            let top = other.pos.y + (reach * reach - dx * dx).squareRoot() * 0.985
            if top > y0 {
                y0 = top
                on = other
            }
        }
        return (y0, on)
    }

    private func advance(_ i: Int, dt: Float, terrain: SnowTerrain) -> Bool {
        var ball = balls[i]
        let r = ball.radius
        let before = ball.pos
        let spot = rest(ball.pos.x, r, y: ball.pos.y, ignoring: ball.id, terrain)
        if ball.airborne {
            ball.vel.y -= Self.gravity * dt
            ball.pos += ball.vel * dt
            if ball.pos.x < -r || ball.pos.x > terrain.width + r || ball.pos.y < -200 {
                terrain.burst(ball.mass * 0.9, SIMD2(min(max(ball.pos.x, 0), terrain.width), 4), .zero)
                balls[i].gone = true
                return true
            }
            let floor = rest(ball.pos.x, r, y: ball.pos.y, ignoring: ball.id, terrain)
            if ball.pos.y <= floor.y && ball.vel.y <= 0 {
                let impact = -ball.vel.y
                if impact > Self.breaks {
                    terrain.burst(ball.mass * 0.9, SIMD2(ball.pos.x, floor.y), SIMD2(ball.vel.x * 0.3, impact * 0.12))
                    balls[i].gone = true
                    return true
                }
                ball.pos.y = floor.y
                ball.vel.y = 0
                ball.airborne = false
            }
            balls[i] = ball
            return true
        }

        // Resting: a ball on top of another stays if it is centred and not the bigger one, else rolls off.
        if let under = spot.on {
            let dx = ball.pos.x - under.pos.x
            let fits = abs(dx) <= under.radius * 0.5 && r <= under.radius * 1.1
            if fits {
                ball.support = under.id
                ball.pos.x += (under.pos.x - ball.pos.x) * min(1, dt * 9)
                ball.vel.x = 0
            } else {
                ball.support = nil
                let side: Float = abs(dx) < 0.5 ? (ball.seed > 0.5 ? 1 : -1) : (dx > 0 ? 1 : -1)
                ball.vel.x += side * 900 * dt
                ball.vel.x *= exp(-dt * 0.8)
                ball.pos.x += ball.vel.x * dt
                ball.roll += ball.vel.x * dt / max(r, 1)
            }
        } else {
            ball.support = nil
            ball.vel.x *= exp(-dt * 6)
            if abs(ball.vel.x) < 2 { ball.vel.x = 0 }
            ball.pos.x += ball.vel.x * dt
            ball.roll += ball.vel.x * dt / max(r, 1)
        }
        // The snow it rests on may have been scraped from under it, or a ball beneath it taken.
        let floor = rest(ball.pos.x, r, y: ball.pos.y, ignoring: ball.id, terrain).y
        if ball.pos.y > floor + 1.5 {
            ball.airborne = true
            ball.support = nil
        } else {
            ball.pos.y = floor
        }
        let moved = simd_distance(ball.pos, before) > 0.12
        ball.settled = moved || ball.airborne ? 0 : ball.settled + dt
        balls[i] = ball
        return moved || ball.airborne
    }

    // MARK: Snowmen

    /// The balls stacked on top of one another, bottom to top.
    private func stacks() -> [[Int]] {
        var result: [[Int]] = []
        for (i, ball) in balls.enumerated() where ball.support == nil && !ball.airborne && ball.id != heldID {
            var chain = [i]
            var next = balls.firstIndex { $0.support == balls[chain.last!].id && !$0.airborne }
            while let n = next, chain.count < 8 {
                chain.append(n)
                next = balls.firstIndex { $0.support == balls[n].id && !$0.airborne }
            }
            result.append(chain)
        }
        return result
    }

    /// Three balls stacked make a snowman: it gets a face once it has stood still a moment.
    private func adorn(clock: Double) {
        var finished = Set<Int>()
        for chain in stacks() where chain.count >= 3 && chain.allSatisfy({ balls[$0].settled > 0.35 }) {
            for i in chain { finished.insert(balls[i].id) }
        }
        for i in balls.indices {
            if finished.contains(balls[i].id) {
                if balls[i].adorned == nil { balls[i].adorned = clock }
            } else {
                balls[i].adorned = nil
            }
        }
    }

    func sprites(clock: Double) -> [BallSprite] {
        var roles: [Int: Float] = [:]
        for chain in stacks() where chain.count >= 3 && chain.allSatisfy({ balls[$0].adorned != nil }) {
            for (k, i) in chain.enumerated() {
                roles[balls[i].id] = k == chain.count - 1 ? 2 : k == 0 ? 0 : 1
            }
        }
        return balls.sorted { $0.pos.y < $1.pos.y }.map { ball in
            let age = Float(clock - (ball.adorned ?? clock))
            return BallSprite(
                center: ball.pos,
                radius: ball.radius,
                seed: ball.seed,
                roll: ball.roll,
                role: roles[ball.id] ?? 0,
                pop: ball.adorned == nil ? 0 : exp(-age * 4.5)
            )
        }
    }
}

// MARK: - Frost

/// Frost growing in from the corners and edges of the screen, drawn as feathery ferns whose growth order is stored in
/// the pattern, and wiped away wherever the pointer passes, to freeze over again slowly.
final class Frost {
    private(set) var width = 0
    private(set) var height = 0
    /// Two bytes a point: how much frost is there, and when in its growth it reaches that point.
    private(set) var pattern: [UInt8] = []
    /// Bumped whenever the pattern is made anew, for the renderer to upload it.
    private(set) var patternVersion = 0
    static let cell = 4
    private(set) var wipeWidth = 0
    private(set) var wipeHeight = 0
    private(set) var wipe: [UInt8] = []
    private(set) var wipeDirty = false
    /// How far the frost has grown, 0 to 1.
    private(set) var progress: Float = 0
    private var shown: Float = 0
    private var regrowAt: Double = 0
    private var wiped = 0
    private var making = false
    private var pointer: SIMD2<Float>?
    private var seed: UInt64 = 0x5EED

    /// Seconds from clear glass to the screen's edges frozen over.
    var duration: Float = {
        if let text = ProcessInfo.processInfo.environment["SNOWDRIFT_FROST_SECONDS"], let value = Float(text), value > 1 { return value }
        return 210
    }()
    /// Seconds for the wiped glass to freeze over again.
    var refreeze: Float = 50
    static let reach: Float = 26

    var enabled = false {
        didSet {
            guard enabled != oldValue else { return }
            if enabled {
                make()
            } else {
                progress = 0
                shown = 0
                wiped = 0
                wipe = Array(repeating: 0, count: wipe.count)
                wipeDirty = true
            }
        }
    }

    /// Frost on the glass, or growing: the screen is drawn.
    var active: Bool { enabled && !pattern.isEmpty }

    func resize(width: Float, height: Float) {
        let w = max(Int(width.rounded()), 1), h = max(Int(height.rounded()), 1)
        guard w != self.width || h != self.height else { return }
        self.width = w
        self.height = h
        wipeWidth = (w + Self.cell - 1) / Self.cell
        wipeHeight = (h + Self.cell - 1) / Self.cell
        wipe = Array(repeating: 0, count: wipeWidth * wipeHeight)
        wiped = 0
        wipeDirty = true
        pattern = []
        if enabled { make() }
    }

    func acknowledgeWipe() { wipeDirty = false }
    func markWipeStale() { wipeDirty = true }

    /// Grows the frost, wipes it where the pointer is and lets the wiped glass freeze over; returns whether what is drawn changed.
    func step(dt: Float, pointer point: SIMD2<Float>?, clock: Double) -> Bool {
        guard enabled, !pattern.isEmpty else { return false }
        var changed = false
        if progress < 1 {
            progress = min(1, progress + dt / duration)
            if progress - shown >= 0.0015 || progress >= 1 && shown < 1 {
                shown = progress
                changed = true
            }
        }
        if let point {
            wipeAround(from: pointer ?? point, to: point)
            pointer = point
        } else {
            pointer = nil
        }
        if wiped > 0 && clock >= regrowAt {
            regrowAt = clock + 0.2
            let step = UInt8(max(1, min(255, Int((255 * 0.2 / refreeze).rounded(.up)))))
            var left = 0
            for i in wipe.indices where wipe[i] > 0 {
                wipe[i] = wipe[i] > step ? wipe[i] - step : 0
                if wipe[i] > 0 { left += 1 }
            }
            wiped = left
            wipeDirty = true
        }
        return changed || wipeDirty
    }

    private func wipeAround(from a: SIMD2<Float>, to b: SIMD2<Float>) {
        let distance = simd_distance(a, b)
        let steps = max(1, Int(distance / (Self.reach * 0.4)))
        for k in 1...steps {
            let p = a + (b - a) * (Float(k) / Float(steps))
            stamp(p)
        }
    }

    /// The pointer at a point in overlay coordinates, whose y runs up from the bottom of the screen.
    private func stamp(_ p: SIMD2<Float>) {
        let cell = Float(Self.cell)
        let cx = p.x / cell, cy = (Float(height) - p.y) / cell
        let radius = Self.reach / cell
        let x0 = max(Int((cx - radius).rounded(.down)), 0), x1 = min(Int((cx + radius).rounded(.up)), wipeWidth - 1)
        let y0 = max(Int((cy - radius).rounded(.down)), 0), y1 = min(Int((cy + radius).rounded(.up)), wipeHeight - 1)
        guard x0 <= x1, y0 <= y1 else { return }
        for y in y0...y1 {
            for x in x0...x1 {
                let d = simd_distance(SIMD2(Float(x) + 0.5, Float(y) + 0.5), SIMD2(cx, cy)) / radius
                guard d < 1 else { continue }
                let t = min(max((1 - d) / 0.45, 0), 1)
                let amount = UInt8(t * t * (3 - 2 * t) * 255)
                let i = y * wipeWidth + x
                if amount > wipe[i] {
                    if wipe[i] == 0 { wiped += 1 }
                    wipe[i] = amount
                    wipeDirty = true
                }
            }
        }
    }

    private func make() {
        guard !making, width > 0, height > 0 else { return }
        making = true
        let (w, h, seed) = (width, height, self.seed)
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let made = FrostPattern.make(width: w, height: h, seed: seed)
            DispatchQueue.main.async {
                guard let self else { return }
                self.making = false
                guard w == self.width, h == self.height else {
                    if self.enabled { self.make() }
                    return
                }
                self.pattern = made
                self.patternVersion += 1
                self.wipeDirty = true
            }
        }
    }
}

/// The ferns of frost: stems with feathers on both sides, themselves feathered, drawn from the corners and the middles
/// of the edges toward the middle of the screen.
enum FrostPattern {
    private struct Source {
        var state: UInt64
        mutating func next() -> Float {
            state &+= 0x9E3779B97F4A7C15
            var z = state
            z = (z ^ (z >> 30)) &* 0xBF58476D1CE4E5B9
            z = (z ^ (z >> 27)) &* 0x94D049BB133111EB
            z ^= z >> 31
            return Float(z >> 40) / Float(1 << 24)
        }
        mutating func range(_ low: Float, _ high: Float) -> Float { low + (high - low) * next() }
    }

    private struct Canvas {
        let width: Int
        let height: Int
        var bytes: [UInt8]
        /// Path length that reads as the end of the growth.
        let reach: Float

        /// A soft dot of the given radius; where dots overlap the stronger one stays, with its own arrival time.
        mutating func dot(_ x: Float, _ y: Float, _ radius: Float, arrival: Float) {
            let r = radius + 1
            let x0 = max(Int((x - r).rounded(.down)), 0), x1 = min(Int((x + r).rounded(.up)), width - 1)
            let y0 = max(Int((y - r).rounded(.down)), 0), y1 = min(Int((y + r).rounded(.up)), height - 1)
            guard x0 <= x1, y0 <= y1 else { return }
            let time = UInt8(min(max(arrival / reach, 0), 1) * 255)
            for py in y0...y1 {
                for px in x0...x1 {
                    let d = ((Float(px) + 0.5 - x) * (Float(px) + 0.5 - x) + (Float(py) + 0.5 - y) * (Float(py) + 0.5 - y)).squareRoot()
                    let cover = min(max(radius + 0.5 - d, 0), 1)
                    guard cover > 0 else { continue }
                    let i = (py * width + px) * 2
                    let value = UInt8(cover * 255)
                    if value > bytes[i] {
                        bytes[i] = value
                        bytes[i + 1] = time
                    } else if value > 0 && time < bytes[i + 1] && bytes[i] > 40 {
                        bytes[i + 1] = time
                    }
                }
            }
        }
    }

    /// Draws one frond growing from a point: a curving stem with feathers on both sides that shorten toward the tip.
    private static func frond(_ canvas: inout Canvas, _ random: inout Source, from origin: SIMD2<Float>, angle start: Float, length: Float, curl: Float,
                              thickness: Float, startPath: Float, depth: Int) {
        let step: Float = 1.1
        let steps = max(Int(length / step), 1)
        var position = origin
        var angle = start
        let spacing = depth == 0 ? 5 : depth == 1 ? 5 : 6
        var path = startPath
        for s in 0..<steps {
            let along = Float(s) / Float(steps)
            angle += curl / Float(steps)
            position += SIMD2(cos(angle), sin(angle)) * step
            path += step
            let radius = max(0.45, thickness * (1 - 0.55 * along))
            canvas.dot(position.x, position.y, radius, arrival: path)
            guard depth < 2, s % spacing == 2, along > 0.03, along < 0.97 else { continue }
            for side: Float in [-1, 1] {
                let feather = length * (depth == 0 ? 0.3 : 0.4) * pow(1 - along, 0.75) * random.range(0.8, 1.15)
                guard feather > 5 else { continue }
                let spread = 1.18 - 0.5 * along
                frond(&canvas, &random, from: position, angle: angle + side * spread, length: feather, curl: -side * random.range(0.05, 0.45),
                      thickness: thickness * 0.55, startPath: path, depth: depth + 1)
            }
        }
    }

    /// `width` by `height` points, y running down as in a picture; the renderer flips it onto the screen.
    static func make(width: Int, height: Int, seed: UInt64) -> [UInt8] {
        var random = Source(state: seed)
        let big = Float(min(width, height))
        let reach = big * 1.15
        var canvas = Canvas(width: width, height: height, bytes: [UInt8](repeating: 0, count: width * height * 2), reach: reach)
        let w = Float(width), h = Float(height)

        // Corners: four fronds fanning out toward the middle of the screen.
        let corners: [(SIMD2<Float>, Float)] = [
            (SIMD2(-2, -2), .pi / 4), (SIMD2(w + 2, -2), .pi * 3 / 4),
            (SIMD2(-2, h + 2), -.pi / 4), (SIMD2(w + 2, h + 2), -.pi * 3 / 4),
        ]
        for (origin, direction) in corners {
            for offset: Float in [-0.62, -0.2, 0.2, 0.62] {
                frond(&canvas, &random, from: origin, angle: direction + offset + random.range(-0.08, 0.08), length: big * random.range(0.5, 0.78),
                      curl: random.range(-0.5, 0.5), thickness: random.range(1.5, 2.0), startPath: random.range(0, reach * 0.12), depth: 0)
            }
        }
        // Edges: three fronds from a point along each, leaning in.
        let edges: [(SIMD2<Float>, Float)] = [
            (SIMD2(w * random.range(0.35, 0.65), -2), .pi / 2), (SIMD2(w * random.range(0.35, 0.65), h + 2), -.pi / 2),
            (SIMD2(-2, h * random.range(0.35, 0.65)), 0), (SIMD2(w + 2, h * random.range(0.35, 0.65)), .pi),
        ]
        for (origin, direction) in edges {
            for offset: Float in [-0.55, 0, 0.55] {
                frond(&canvas, &random, from: origin, angle: direction + offset + random.range(-0.1, 0.1), length: big * random.range(0.3, 0.5),
                      curl: random.range(-0.4, 0.4), thickness: random.range(1.2, 1.7), startPath: random.range(0, reach * 0.18), depth: 0)
            }
        }
        // Smaller fronds along the edges between.
        for _ in 0..<10 {
            let side = Int(random.range(0, 4)) % 4
            let t = random.range(0.08, 0.92)
            let (origin, direction): (SIMD2<Float>, Float) = switch side {
            case 0: (SIMD2(w * t, -2), .pi / 2)
            case 1: (SIMD2(w * t, h + 2), -.pi / 2)
            case 2: (SIMD2(-2, h * t), 0)
            default: (SIMD2(w + 2, h * t), .pi)
            }
            frond(&canvas, &random, from: origin, angle: direction + random.range(-0.7, 0.7), length: big * random.range(0.12, 0.26),
                  curl: random.range(-0.5, 0.5), thickness: random.range(1.0, 1.4), startPath: random.range(reach * 0.05, reach * 0.3), depth: 0)
        }
        return canvas.bytes
    }
}
