// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::crc32c_with_seed;

fn scalar_crc(bytes: &[u8], seed: u32) -> u32 {
    let mut crc = !seed;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f6_3b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

// rust-internal: accelerated and portable checksum paths must preserve the existing seeded codec.
#[test]
fn record_checksums_match_scalar_at_alignment_and_dispatch_boundaries() {
    let bytes: Vec<u8> = (0..131080)
        .map(|i| ((i * 37 + i / 257) % 256) as u8)
        .collect();
    for offset in 0..8 {
        for length in [
            0, 1, 7, 8, 9, 31, 32, 63, 64, 127, 128, 255, 256, 511, 512, 1023, 1024, 4095, 4096,
            65535, 131072,
        ] {
            let payload = &bytes[offset..offset + length];
            for seed in [0, 1, 0x1234_5678, u32::MAX] {
                let expected = scalar_crc(payload, seed);
                assert_eq!(
                    crc32c_with_seed(payload, seed),
                    expected,
                    "offset={offset}, length={length}, seed={seed}"
                );
                let split = length / 3;
                assert_eq!(
                    crc32c_with_seed(&payload[split..], crc32c_with_seed(&payload[..split], seed)),
                    expected
                );
            }
        }
    }
}
