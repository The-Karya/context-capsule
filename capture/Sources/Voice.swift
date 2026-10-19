// context-capsule — capture: the mic + Apple Speech, fully on this Mac (requiresOnDeviceRecognition).
// The mic is open only between the two presses. Audio buffers go straight to the recognizer in
// memory; no audio is ever written to disk, and nothing is sent to Apple's servers.

import AVFoundation
import CoreAudio
import Speech

final class SpeechEngine: VoiceEngine {
    private let audio = AVAudioEngine()
    private var recognizer: SFSpeechRecognizer?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var tapped = false
    private let lock = NSLock()
    private var peak: Float = 0
    private var frames: AVAudioFramePosition = 0
    private var sampleRate: Double = 0
    private var transcript = ""
    private var done: ((Result<VoiceClip, Error>) -> Void)?
    private var finalTimer: DispatchWorkItem?
    private var ended = false // the recognizer already stopped on its own (an error mid-note)

    struct Failure: LocalizedError {
        var errorDescription: String?
        init(_ s: String) { errorDescription = s }
    }

    var deviceName: String? { Self.defaultInputName() }

    // MARK: VoiceEngine

    func prepare(locale: Locale, _ done: @escaping (VoiceOutcome?) -> Void) {
        Self.speechAllowed { speechOK in
            guard speechOK else { return done(.denied(.speech)) }
            Self.micAllowed { micOK in
                guard micOK else { return done(.denied(.microphone)) }
                guard let recognizer = SFSpeechRecognizer(locale: locale), recognizer.supportsOnDeviceRecognition else {
                    return done(.unavailable(locale: locale.identifier))
                }
                guard recognizer.isAvailable else { return done(.failed("speech recognition is busy, try again")) }
                self.recognizer = recognizer
                done(nil)
            }
        }
    }

    func start() throws {
        guard let recognizer else { throw Failure("speech recognition is not ready") }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.requiresOnDeviceRecognition = true // never Apple's servers
        request.shouldReportPartialResults = true
        request.addsPunctuation = true

        let input = audio.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw Failure("no microphone found") }
        lock.lock()
        peak = 0
        frames = 0
        sampleRate = format.sampleRate
        transcript = ""
        done = nil
        ended = false
        lock.unlock()

        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self, weak request] buffer, _ in
            request?.append(buffer) // memory only
            self?.measure(buffer)
        }
        tapped = true
        self.request = request
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            self?.recognized(result, error)
        }
        audio.prepare()
        do { try audio.start() } catch {
            cancel()
            throw Failure("the microphone could not start (\(error.localizedDescription))")
        }
    }

    func finish(_ done: @escaping (Result<VoiceClip, Error>) -> Void) {
        closeMic()
        lock.lock()
        self.done = done
        let already = ended
        lock.unlock()
        if already { return complete(nil) }
        request?.endAudio()
        // The final result normally arrives within a second; never wait forever.
        let timer = DispatchWorkItem { [weak self] in self?.complete(nil) }
        finalTimer = timer
        DispatchQueue.main.asyncAfter(deadline: .now() + 10, execute: timer)
    }

    func cancel() {
        closeMic()
        task?.cancel()
        reset()
    }

    // MARK: inside

    private func closeMic() {
        if audio.isRunning { audio.stop() }
        if tapped { audio.inputNode.removeTap(onBus: 0); tapped = false }
    }

    private func reset() {
        finalTimer?.cancel()
        finalTimer = nil
        task = nil
        request = nil
        lock.lock()
        done = nil
        transcript = ""
        lock.unlock()
    }

    private func measure(_ buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData else { return }
        var p: Float = 0
        let n = Int(buffer.frameLength)
        for c in 0..<Int(buffer.format.channelCount) {
            p = max(p, Voice.peak(UnsafeBufferPointer(start: channels[c], count: n)))
        }
        lock.lock()
        peak = max(peak, p)
        frames += AVAudioFramePosition(n)
        lock.unlock()
    }

    private func recognized(_ result: SFSpeechRecognitionResult?, _ error: Error?) {
        if let result {
            lock.lock()
            transcript = result.bestTranscription.formattedString
            lock.unlock()
            if result.isFinal { complete(nil) }
        } else if let error {
            complete(error)
        }
    }

    /// Hand back the clip once: on the final result, an error, or the 10 s guard.
    private func complete(_ error: Error?) {
        lock.lock()
        let done = self.done
        self.done = nil
        let clip = VoiceClip(text: transcript, peak: peak, seconds: sampleRate > 0 ? Double(frames) / sampleRate : 0)
        if done == nil { ended = true }
        lock.unlock()
        guard let done else { return } // not finishing yet (a mid-note error waits for finish)
        DispatchQueue.main.async { self.reset() }
        // "No speech detected" is not a failure: it is an empty clip.
        if let error, !Self.isNoSpeech(error), clip.text.isEmpty { done(.failure(error)) } else { done(.success(clip)) }
    }

    private static func isNoSpeech(_ error: Error) -> Bool {
        let e = error as NSError
        return (e.domain == "kAFAssistantErrorDomain" && [203, 1110].contains(e.code))
            || e.domain == "kLSRErrorDomain"
    }

    // MARK: permissions

    private static func speechAllowed(_ done: @escaping (Bool) -> Void) {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized: done(true)
        case .notDetermined: SFSpeechRecognizer.requestAuthorization { done($0 == .authorized) }
        default: done(false)
        }
    }

    private static func micAllowed(_ done: @escaping (Bool) -> Void) {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: done(true)
        case .notDetermined: AVCaptureDevice.requestAccess(for: .audio, completionHandler: done)
        default: done(false)
        }
    }

    /// The default input device's name (e.g. "MacBook Air Microphone").
    private static func defaultInputName() -> String? {
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultInputDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &id) == noErr
        else { return nil }
        var name: Unmanaged<CFString>?
        size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        address.mSelector = kAudioObjectPropertyName
        guard AudioObjectGetPropertyData(id, &address, 0, nil, &size, &name) == noErr, let name else { return nil }
        return name.takeRetainedValue() as String
    }
}
