import AppKit
import Foundation
import Vision

let data = FileHandle.standardInput.readDataToEndOfFile()
guard let image = NSImage(data: data),
      let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else { exit(2) }
let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.recognitionLanguages = ["zh-Hans", "en-US"]
request.usesLanguageCorrection = false
request.minimumTextHeight = 0.004
try VNImageRequestHandler(cgImage: cg).perform([request])
let rows: [[String: Any]] = (request.results ?? []).compactMap { observation in
    guard let candidate = observation.topCandidates(1).first else { return nil }
    let box = observation.boundingBox
    return ["text": candidate.string, "confidence": candidate.confidence,
            "x": Int((box.midX * CGFloat(cg.width)).rounded()),
            "y": Int(((1 - box.midY) * CGFloat(cg.height)).rounded())]
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: rows))
