// tools/generate_nfkc_fixture.swift
//
// Re-measures `crates/subtext-core/tests/fixtures/nfkc-case-fold.tsv`.
//
// The fixture's first column is the corpus; the second is what Foundation
// produces for it. Only the second column is rewritten, so the inputs stay put
// and the expectations follow whatever Unicode version the Swift toolchain
// carries.
//
// NFKC Case-Fold is not one call in Foundation — upstream composes it from four,
// in `String+Normalization.swift`, and this repeats that composition rather than
// approximating it: remove default-ignorables, NFKC, case fold, NFC, remove
// default-ignorables again. The removal uses `.unicodeScalar` matching
// semantics, because the default grapheme-cluster semantics would leave a ZWJ
// glued to the character before it and never see it.
//
// Usage:
//
//     swift tools/generate_nfkc_fixture.swift \
//         crates/subtext-core/tests/fixtures/nfkc-case-fold.tsv

import Foundation

extension String {

    /// `precomposedStringWithCompatibilityMappingWithCaseFold`, upstream's name
    /// for NFKC Case-Fold.
    var nfkcCaseFold: String {
        let ignorable = try! Regex("\\p{Default_Ignorable_Code_Point}+")
            .matchingSemantics(.unicodeScalar)

        return self.replacing(ignorable, with: "")
            .precomposedStringWithCompatibilityMapping
            .folding(options: .caseInsensitive, locale: nil)
            .precomposedStringWithCanonicalMapping
            .replacing(ignorable, with: "")
    }
}

let arguments = CommandLine.arguments
guard arguments.count == 2 else {
    FileHandle.standardError.write("usage: gen_nfkc_fixture <fixture.tsv>\n".data(using: .utf8)!)
    exit(2)
}

let path = arguments[1]
let text = try String(contentsOfFile: path, encoding: .utf8)

var out: [String] = []
var measured = 0
for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
    if line.isEmpty || line.hasPrefix("#") {
        out.append(String(line))
        continue
    }
    let input = line.split(separator: "\t", omittingEmptySubsequences: false)
        .first.map(String.init) ?? ""
    let scalars = input.split(separator: " ")
        .compactMap { UInt32($0, radix: 16) }
        .compactMap { Unicode.Scalar($0) }
    let folded = String(String.UnicodeScalarView(scalars)).nfkcCaseFold
    let encoded = folded.unicodeScalars
        .map { String(format: "%X", $0.value) }
        .joined(separator: " ")
    out.append("\(input)\t\(encoded)")
    measured += 1
}

try out.joined(separator: "\n").write(toFile: path, atomically: true, encoding: .utf8)
print("re-measured \(measured) rows in \(path)")
