# Nabi Synth

## Live performance synthesizer based on midi_fundsp. Designed to run on low-mid cost 64-bit SBCs.

Features:
* TOML defined patches with touch-sensitive and polyphonic synth engines based on FunDSP, with optional CC control with a split between FX controls and Sound Controls (e.g., 4 top encoders vs. 4 bottom encoders).
* Onboarding script for mappig your controller based on the above design philosophy.
* Master effects chain.
* Support for sending SysEx commands to your controllers screen (when possible).
* Tunning functions for micro/macro tonality (Scala file support is planned).
* Easy to hack and extend (Youtube tutorials are planned).
* Wiki with guides on how to install on an SBC (Tested on a Libre Computer Le Potato - if you want to try on a different board please open an issue or reach out on discord)

Synthesis engines:
Morph2: 2 Morphing oscillators (each with A osc and B osc) with optional FM (A>B) and detune.
SuperOSC: Detuned oscillators with oscillator count and spread control. Great for supersaws.
KrS: Comb-filtered Karplus-Strong physical modeling with polarity and excitation noise selection, as well as attack envelope on excitation and dampening control for string decay.
DrOrgan: volume control over 8 "draws" each with its own ratio relative to root.

All Oscillator selections feature all the common types as well as an optional wavetable (configurable via a path to a local wave file).

Master Effects:
Stereo Reverb: FunDSPs stereo reverb, with all values controlled via CC and up to 10 seconds of reverb (use at your own caution! )
EQ2: Low and High cut with separate Qs.
Tape style pitch drift.
LoFi pitch-shifter inspired by Bitwig's pitch shifter.
Single delay line with EQ2 applied to the wet signal.

Distortions and bit-crushers are also planned. 

More effects are planned! If you are missing a feature feel free to open an issue.


All Contributions to this repo must adhere to the rust audio community AI policy:
https://rust.audio/community/ai/

