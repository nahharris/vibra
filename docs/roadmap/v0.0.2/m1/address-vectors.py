"""Recheck the published M1 address-vector literals independently of Rust."""

from __future__ import annotations

import hashlib
import re
from pathlib import Path


COMPONENT_DOMAIN = b"vibra-codebase-component-v1\0"
OBJECT_DOMAIN = b"vibra-codebase-object-v1\0"
FIXTURE = Path(__file__).with_suffix(".md")

EXPECTED = (
    (
        "sha256:007833aa1b06fbade3d0e75485111bc6ebc39fbe1859f4959ce2161feac3aa03",
        ("sha256:e961613b65f10343533506c1add52cdb85098bab87c31fcfbf1958ce3cd95613",),
    ),
    (
        "sha256:8130d40260251f9dbfd50e21c3472a85051ab965d8580f70fda4b8291787e64d",
        ("sha256:cf09809d2ed35d14728846c1f67bc028be7582026f736470fe2e42ccb03192e0",),
    ),
    (
        "sha256:5410603ea9a48013bfb58a5690772dc3319a9a0b8b80ddec265d523337f304b3",
        (
            "sha256:3c04ec7243737de86c0594671c76dab58fa04fe36ee85a94545b94a9c61725b7",
            "sha256:0398214f07198493243573b82e17ec590a2311d8fcf96eda5edf105ba66e8860",
        ),
    ),
    (
        "sha256:962b4e74a3342bf941323d295a287021eff4712cafe143b7684d81b9b4256e80",
        ("sha256:3a3a15f1cb4ccfb35d934db447887e6694a3fcce03df87384b2894f07658a956",),
    ),
    (
        "sha256:415658873a6bd2644666d1cb1cb05004f612f92f82787b5d658160c794b3dc48",
        ("sha256:bb49699bb68f60e7a38c8cfa82588baeaf0150e6e15fde6618e8ee6478b72387",),
    ),
    (
        "sha256:732e7da21f6e3811f918f21eccc1f7c825383e11201197d5ad98f1a6ebb58ec7",
        ("sha256:5d9f91cb7e1b8c1cfdfee99a45cffb6c2d232d6a9a404ee53476bbe9583d8bf8",),
    ),
)


def address(digest: bytes) -> str:
    return f"sha256:{digest.hex()}"


def main() -> None:
    text = FIXTURE.read_text(encoding="utf-8")
    frames = re.findall(r"~~~vibon\n(.*?)\n~~~", text, re.DOTALL)
    if len(frames) != len(EXPECTED):
        raise SystemExit(f"expected {len(EXPECTED)} frames, found {len(frames)}")

    for index, (frame_text, (expected_component, expected_members)) in enumerate(
        zip(frames, EXPECTED, strict=True), start=1
    ):
        frame = (frame_text + "\n").encode("utf-8")
        digest = hashlib.sha256(COMPONENT_DOMAIN + frame).digest()
        actual_component = address(digest)
        if actual_component != expected_component:
            raise SystemExit(
                f"frame {index}: component address {actual_component} != {expected_component}"
            )

        actual_members = tuple(
            address(
                hashlib.sha256(
                    OBJECT_DOMAIN + digest + ordinal.to_bytes(8, byteorder="big")
                ).digest()
            )
            for ordinal in range(len(expected_members))
        )
        if actual_members != expected_members:
            raise SystemExit(
                f"frame {index}: object addresses {actual_members} != {expected_members}"
            )
        print(f"{index}: {actual_component} ({len(actual_members)} member(s))")


if __name__ == "__main__":
    main()
