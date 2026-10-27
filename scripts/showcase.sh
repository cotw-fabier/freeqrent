#!/bin/sh
# Renders the README's showcase codes into docs/showcase/ and verifies that
# each one decodes before it's written. Rerun after any output change.
# Every intro replays every 5 s, so readers who scroll down still see one.
set -eu
cargo build -q --release -p freeqrent-cli
q="$(cargo metadata -q --format-version 1 --no-deps | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')/release/freeqrent"
out=docs/showcase
logos=$out/logos
pictures=$out/pictures

run() {
    name=$1
    shift
    echo "== $name"
    "$q" "$@" --trigger every:5000 --verify -o "$out/$name.svg"
}

run hi-mom "Hi Mom!" --shape liquid:0.85 --finder squircle/circle \
    --bg "#fff2f6" --fg "linear-gradient(135deg, oklch(36% .17 355), oklch(29% .15 300))" \
    --token finder.eye="oklch(34% .18 15)" \
    --logo $logos/heart.svg --logo-shape circle --logo-frame ring --logo-intro stamp \
    --intro ripple --intro-ms 1400 --idle breathe --label "Hi Mom!"

run secret-fee 'Secret $4 Fee Not Included' --shape dots:0.86 --finder rounded:1.5/circle \
    --bg "#f3fbf4" --fg "linear-gradient(160deg, oklch(33% .1 160), oklch(28% .08 220))" \
    --token finder.eye="oklch(34% .09 85)" \
    --logo $logos/coin.svg --logo-shape circle --logo-frame orbit --logo-intro spin \
    --intro sweep --intro-ms 1300 --idle shimmer --idle logo-spin \
    --label 'Secret $4 Fee Not Included'

run rusty "Built with Claude because my programming skills got RUSTy (get it?)" \
    --shape rounded:0.45 --finder squircle/squircle --frame circle:0.5 --frame-ring dashed \
    --bg "#fff6ee" --fg "radial-gradient(oklch(36% .13 40), oklch(26% .08 30))" \
    --token finder.eye="oklch(35% .14 45)" --token filler="oklch(36% .13 40 / .3)" \
    --logo $logos/gear.svg --logo-scale 0.24 --logo-shape circle --logo-frame beads \
    --intro assemble --intro-ms 1600 --idle finder-pulse --idle logo-turn \
    --label "Built with Claude because my programming skills got RUSTy (get it?)"

run secret-level "You found the secret level. There is no secret level." \
    --shape square:0.12 --finder square/square --timing dashed \
    --bg "#0b1410" --fg "linear-gradient(90deg, #7dffa8, #6fe3ff)" --token finder.eye="#c6ff7d" \
    --allow-inverted --intro raster --intro-ms 1500 --idle shimmer \
    --label "You found the secret level. There is no secret level."

run achievement "Achievement unlocked: pointed phone at square" \
    --shape bars-v:0.5 --finder rounded:1/rounded:0.8 \
    --bg "#fff8ef" --fg "linear-gradient(135deg, oklch(37% .16 30), oklch(33% .17 330), oklch(30% .15 285))" \
    --intro iris --intro-ms 1400 --stagger polar --idle finder-pulse \
    --label "Achievement unlocked: pointed phone at square"

run hug "This QR code is legally a hug" \
    --shape dots:0.82 --finder rounded:1.5/circle --frame circle:0.5 --frame-ring solid \
    --bg "#f1f6ff" --fg "linear-gradient(135deg, oklch(34% .14 265), oklch(31% .13 200))" \
    --token filler="oklch(34% .12 240 / .3)" --token finder.eye="oklch(36% .17 300)" \
    --intro settle --intro-ms 1500 --stagger radial --idle breathe \
    --label "This QR code is legally a hug"

# Art mode: pictures built into the code, on the sparse grid so phones read
# them quickly.
run art-penguin "Dress code: black tie. Species: penguin." \
    --art $pictures/penguin.jpg --ink color --tracking-grid sparse \
    --intro develop --intro-ms 1800 \
    --label "Dress code: black tie. Species: penguin."

run art-samurai "Forged in Rust. Folded 1,000 times." \
    --art $pictures/samurai.jpg --ink mono --tracking-grid sparse \
    --fg "linear-gradient(160deg, oklch(28% .09 280), oklch(32% .14 20))" \
    --intro sweep --intro-ms 1400 \
    --label "Forged in Rust. Folded 1,000 times."

run art-windmill "Not all who wander are lost. Some are just scanning." \
    --art $pictures/windmill.jpg --ink color --tracking-grid sparse \
    --intro ripple --intro-ms 1600 --idle shimmer \
    --label "Not all who wander are lost. Some are just scanning."

# Design systems from the gallery, on the M7.5 chrome: borders, shadows,
# effects, textures and captions.
run maximalism "More is more. This QR code has never heard of restraint." \
    --allow-inverted --bg "#0d0d1a" --fg "#00f5d4" \
    --token fg.2="#ffe600" --token fg.3="#ff77ff" --spread regions \
    --token finder.eye="#ff77ff" --token filler="#ff3af2" --token frame="#ff6b35" \
    --shape liquid:1 --finder leaf/circle --frame circle:0.7 --frame-ring dashed \
    --effect glow:0.3:0.2 --intro ripple --intro-ms 1600 --stagger random --groups 10 \
    --label "More is more. This QR code has never heard of restraint."

run neo-brutalism "SCAN ME. Yes, you. No rounded corners were harmed." \
    --bg "#ffd93d" --fg "#000000" --shape square --timing dashed \
    --border solid:1 --shadow 1.5 --caption "SCAN ME" --caption-tracking 10 \
    --intro assemble --intro-ms 1300 --stagger diagonal \
    --label "SCAN ME. Yes, you. No rounded corners were harmed."

run terminal '$ sudo scan --this-code. Permission granted.' \
    --preset terminal --token finder.eye="#ffb000" \
    --effect glow:0.25 --texture scanlines:0.2 \
    --caption '$ sudo scan_' --caption-font mono --caption-size 1.5 --token caption="#ffb000" \
    --intro raster --intro-ms 1500 --stagger linear \
    --label '$ sudo scan --this-code. Permission granted.'
