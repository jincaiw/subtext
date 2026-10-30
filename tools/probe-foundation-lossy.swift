//
//  probe-foundation-lossy.swift
//
//  What Foundation writes where an encoding cannot spell a character.
//
//      swift tools/probe-foundation-lossy.swift
//
//  This is a question, not a build step. `docs/PLAN.md` §22.49 records the
//  answer — U+003F for every encoding tried — because the *Incompatible
//  Characters* inspector shows that replacement in its Converted column, and
//  CotEditor gets it from Foundation. The gap table used to say "`?` / `＼`";
//  it was wrong, and the only way to know was to ask.
//
//  Re-run this if a new encoding joins the list, or if Foundation changes its
//  mind: the column has to follow whatever this prints.
//

import Foundation

let sample = "Aあ漢é€😀"
let encodings: [(String, String.Encoding)] = [
    ("shiftJIS", .shiftJIS), ("eucJP", .japaneseEUC), ("latin1", .isoLatin1),
    ("cp1252", .windowsCP1252), ("ascii", .ascii), ("utf8", .utf8),
]

for (name, encoding) in encodings {
    guard let data = sample.data(using: encoding, allowLossyConversion: true),
          let back = String(data: data, encoding: encoding)
    else {
        print("\(name): cannot represent the sample at all")
        continue
    }
    let scalars = back.unicodeScalars
        .map { String(format: "U+%04X", $0.value) }
        .joined(separator: " ")
    print("\(name): \(back)   [\(scalars)]")
}
