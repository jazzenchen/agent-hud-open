import CoreGraphics
import QuartzCore

/// A Claude mark's bot drawn from Clawd's own sprites, inside a `BotActor`'s layer: what its `ClawdPerformer` says to
/// draw, baked in whole device pixels (`ClawdBaker`) and shown just as it is, never scaled, turned or morphed.
///
/// The logo and Clawd fade into each other instead. Two layers take turns: one shows the picture, and the other the
/// picture it is fading out of. A frame is handed to the layer only when the picture changes.
@MainActor
final class ClawdActor {
    /// How long the logo and Clawd take to fade into each other.
    static let fade: TimeInterval = 0.3
    /// A mark that has rested this long as its logo lets its kept frames go.
    static let keepsFrames: TimeInterval = 60

    private(set) var performer: ClawdPerformer
    let baker: ClawdBaker
    /// The layer Clawd is shown in, the actor's own, whose opacity and visibility this decides.
    let layer: CALayer
    /// The picture, and the one it fades out of, both over the whole of `layer` and on whole device pixels.
    let front = CALayer(), back = CALayer()
    private let form: BotForm

    /// What can be seen of the mark: a frame of Clawd, or the logo wearing a sign or none; nil while it is not known,
    /// as when Clawd takes over from the drawn bot.
    private struct Seen: Equatable {
        var picture: ClawdPicture?
        var sign: BotSign?
        static let logo = Seen()
    }

    private var seen: Seen?
    /// The frame in `front` is drawn for a size or colours that have changed.
    private var stale = false
    /// When the fade under way ends.
    private var fadesUntil: TimeInterval?
    /// Since when the mark has rested as its logo, its visits as its bot counted in: a calm mark visits more often than
    /// once a minute.
    private var restingSince: TimeInterval?
    /// The mark was out before this actor was built for it, and takes up its strength at once: nothing has changed.
    private var resumes: Bool
    /// The logo's frames, by the sign they wear, for the size and colours they were drawn for.
    private var logos: [BotSign?: CGImage] = [:]

    var sheet: ClawdSheet { baker.sheet }

    /// - showing: whether the mark is shown in `layer` now, by the drawn bot. Otherwise what can be seen of it is its logo,
    ///   from which Clawd fades in.
    /// - resumes: the mark was out before this actor was built for it, as a dock's are when it is laid out again with
    ///   bots on: it shows as it was, with nothing to fade from.
    init(sheet: ClawdSheet, form: BotForm, staysBot: Bool, arrivesAfter: TimeInterval, seed: UInt64, in layer: CALayer,
         showing: Bool, resumes: Bool = false) {
        performer = ClawdPerformer(sheet: sheet, staysBot: staysBot, arrivesAfter: arrivesAfter, seed: seed)
        baker = ClawdBaker(sheet: sheet, paintSign: ClawdBaker.signs)
        self.form = form
        self.layer = layer
        self.resumes = resumes
        seen = showing || resumes ? nil : .logo
        for part in [back, front] {
            part.contentsGravity = .center
            part.magnificationFilter = .nearest
            part.minificationFilter = .nearest
            part.anchorPoint = .zero
            // A fade is added by hand; any other change is a frame of its own.
            part.actions = ["contents": NSNull(), "opacity": NSNull(), "hidden": NSNull(), "bounds": NSNull(),
                            "position": NSNull()]
            layer.addSublayer(part)
        }
        back.isHidden = true
    }

    /// Takes Clawd's layers out of the actor's.
    func remove() {
        front.removeFromSuperlayer()
        back.removeFromSuperlayer()
    }

    func set(_ state: BotState, at now: TimeInterval) { performer.set(state, at: now) }

    /// Takes up `state` at its loop's start, as a Clawd that was already out (`ClawdPerformer.resume`).
    func resume(_ state: BotState, at now: TimeInterval) { performer.resume(state, at: now) }

    func play(_ event: BotEvent, at now: TimeInterval) { performer.play(event, at: now) }

    /// The frames already drawn are for another size or other colours.
    func redraw() {
        stale = true
        logos = [:]
    }

    /// Advances to `now` and shows what Clawd looks like then. Returns the seconds until the picture next changes, or until
    /// something else is due: a fade's end, or the kept frames let go; nil once the picture holds until something is asked.
    ///
    /// - restsHidden: the mark's own logo is drawn under the layer, which hides while that is all there is to see.
    func step(to now: TimeInterval, side: CGFloat, scale: CGFloat, outline: Int, space: CGColorSpace?, restsHidden: Bool,
              aim: () -> CGPoint) -> TimeInterval? {
        defer { resumes = false }
        performer.step(to: now, aim: aim)
        let picture = performer.picture
        let next = Seen(picture: picture, sign: picture == nil ? performer.logoSign : nil)
        // A mark with no news of its agent rests as a faded logo.
        let opacity: Float = picture == nil && performer.state.look == .noNews ? 0.5 : 1
        if let until = fadesUntil, now >= until - BotPace.slack {
            fadesUntil = nil
            back.isHidden = true
            back.contents = nil
        }
        if picture == nil { restingSince = restingSince ?? now } else if !performer.state.look.isCalm { restingSince = nil }
        if picture == nil, let since = restingSince, now - since >= Self.keepsFrames - BotPace.slack, baker.count > 0 {
            baker.purge()
        }

        // Once Clawd has faded into the logo, and the logo back to full strength, the queue's own mark takes over.
        if restsHidden, next == .logo, seen.map({ $0 == .logo }) ?? true, opacity == 1, layer.opacity == 1, fadesUntil == nil {
            if !layer.isHidden { layer.isHidden = true }
            // Under the layer, the mark's own logo is what can be seen.
            seen = .logo
            return wait(from: now)
        }
        let appears = layer.isHidden
        if appears { layer.isHidden = false }
        align(scale: scale)
        if next != seen || stale {
            let image = next.picture.map {
                baker.image($0, side: side, scale: scale, outline: outline, ground: form.ground, space: space)
            } ?? logo(wearing: next.sign, side: side, scale: scale, outline: outline, space: space)
            if let seen, (seen.picture == nil) != (next.picture == nil) {
                // Between the logo and Clawd, the one fades into the other.
                back.contents = seen.picture == nil
                    ? logo(wearing: seen.sign, side: side, scale: scale, outline: outline, space: space) : front.contents
                fade(at: now)
            }
            front.contents = image
            seen = next
            stale = false
        }
        // Shown afresh, the mark was its logo at full strength until now; taken up as it was, it already had this strength.
        if layer.opacity != opacity {
            if resumes { layer.opacity = opacity } else { change(layer, opacity: opacity, from: appears ? 1 : nil, at: now) }
        }
        return wait(from: now)
    }

    /// The seconds until the next thing due: the picture's next change, a fade's end, the kept frames let go.
    private func wait(from now: TimeInterval) -> TimeInterval? {
        var soonest = performer.due
        func take(_ time: TimeInterval?) { if let time { soonest = min(soonest ?? time, time) } }
        take(fadesUntil)
        if performer.picture == nil, let since = restingSince, baker.count > 0 { take(Self.frame(after: since + Self.keepsFrames)) }
        return soonest.map { max(0, $0 - now) }
    }

    /// The first sprite frame of the shared clock at or after `time`, so a wake for anything else lands on one too.
    private static func frame(after time: TimeInterval) -> TimeInterval {
        ((time - BotPace.slack) / BotPace.following).rounded(.up) * BotPace.following
    }

    /// Fades the picture in `back` out and the one in `front` in.
    private func fade(at now: TimeInterval) {
        back.isHidden = false
        for (part, from, to) in [(back, Float(1), Float(0)), (front, Float(0), Float(1))] {
            part.opacity = to
            part.add(Self.fading(from: from, to: to), forKey: "fade")
        }
        fading(until: now + Self.fade)
    }

    /// Fades the whole mark to `opacity`, from `from` or from where it is now.
    private func change(_ layer: CALayer, opacity: Float, from: Float?, at now: TimeInterval) {
        let from = from ?? layer.presentation()?.opacity ?? layer.opacity
        layer.opacity = opacity
        layer.add(Self.fading(from: from, to: opacity), forKey: "opacity")
        fading(until: now + Self.fade)
    }

    private func fading(until end: TimeInterval) {
        fadesUntil = max(fadesUntil ?? end, Self.frame(after: end))
    }

    private static func fading(from: Float, to: Float) -> CABasicAnimation {
        let fading = CABasicAnimation(keyPath: "opacity")
        fading.fromValue = from
        fading.toValue = to
        fading.duration = fade
        fading.timingFunction = IslandAnimation.mediaCurve
        return fading
    }

    /// Lays Clawd's layers over the whole of the actor's, with their corner on a whole device pixel of the window, so a
    /// sprite pixel lands on device pixels however the mark itself is placed.
    private func align(scale: CGFloat) {
        let corner = layer.convert(CGPoint.zero, to: nil)
        let snapped = CGPoint(x: (corner.x * scale).rounded() / scale, y: (corner.y * scale).rounded() / scale)
        let frame = CGRect(origin: layer.convert(snapped, from: nil), size: layer.bounds.size)
        for part in [back, front] {
            if part.contentsScale != scale { part.contentsScale = scale }
            if part.frame != frame { part.frame = frame }
        }
    }

    /// The plain logo, wearing `sign`, in a frame like the sprites'.
    private func logo(wearing sign: BotSign?, side: CGFloat, scale: CGFloat, outline: Int, space: CGColorSpace?) -> CGImage? {
        if let kept = logos[sign] { return kept }
        let image = BotRenderer.logo(form, sign: sign, side: side * scale, outline: outline, space: space)
        logos[sign] = image
        return image
    }
}
