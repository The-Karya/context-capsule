// context-capsule — release: sign or verify latest.json with Ed25519 (CryptoKit).
//   swift release/sign.swift sign   <private-key-file> <latest.json> <latest.json.sig>
//   swift release/sign.swift verify <public-key-base64> <latest.json> <latest.json.sig>
// The private key file holds the raw 32-byte seed in base64. It is read, never printed, never copied.
// The signature file is base64 of the raw 64-byte signature over the exact bytes of latest.json.

import CryptoKit
import Foundation

/// The public key built into the apps. Signing refuses any other key.
let appPublicKey = "WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4="

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("error: \(message)\n".utf8))
    exit(1)
}

let args = CommandLine.arguments
guard args.count == 5 else { fail("usage: sign.swift sign|verify <key> <latest.json> <latest.json.sig>") }
let feedURL = URL(fileURLWithPath: args[3])
let sigURL = URL(fileURLWithPath: args[4])
guard let feed = try? Data(contentsOf: feedURL) else { fail("cannot read \(args[3])") }

switch args[1] {
case "sign":
    guard let text = try? String(contentsOfFile: (args[2] as NSString).expandingTildeInPath, encoding: .utf8),
          let seed = Data(base64Encoded: text.trimmingCharacters(in: .whitespacesAndNewlines)), seed.count == 32,
          let key = try? Curve25519.Signing.PrivateKey(rawRepresentation: seed) else { fail("cannot read the private key") }
    guard key.publicKey.rawRepresentation.base64EncodedString() == appPublicKey else {
        fail("this private key does not match the public key in the apps")
    }
    guard let sig = try? key.signature(for: feed) else { fail("cannot sign") }
    guard key.publicKey.isValidSignature(sig, for: feed) else { fail("the new signature does not verify") }
    do { try Data(sig.base64EncodedString().utf8).write(to: sigURL) } catch { fail("cannot write \(args[4])") }
    print("signed \(feedURL.lastPathComponent)")
case "verify":
    guard let raw = Data(base64Encoded: args[2]), raw.count == 32,
          let key = try? Curve25519.Signing.PublicKey(rawRepresentation: raw) else { fail("bad public key") }
    guard let s = try? String(contentsOf: sigURL, encoding: .utf8),
          let sig = Data(base64Encoded: s.trimmingCharacters(in: .whitespacesAndNewlines)), sig.count == 64 else {
        fail("cannot read the signature")
    }
    guard key.isValidSignature(sig, for: feed) else { fail("signature does NOT match") }
    print("signature ok")
default:
    fail("first argument must be sign or verify")
}
