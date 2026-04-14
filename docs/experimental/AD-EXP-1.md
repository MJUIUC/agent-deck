# AD-EXP-1 — Voice Interface

**Story:** EXP-1 — Voice Interface  
**Status:** 🧪 EXPERIMENTAL — Not on active roadmap. Keeping for reference and future consideration.  
**Branch:** `feature/exp-voice-interface` *(not yet created)*  
**Phase doc reference:** N/A  
**Related stories:** AD-9.8 (Chat UX Polish), iOS Companion App (planned)

---

## Summary

A voice interface layer for agent-deck, enabling fully local speech-to-text (STT) input and text-to-speech (TTS) output without requiring cloud APIs. The goal is a seamless, low-latency voice loop where the user can speak naturally, the agent responds, and audio is played back — all running locally on Apple Silicon hardware.

This document is **experimental**. The ideas here are worth preserving and may inform future development, but this feature is not scheduled for implementation. Revisit when the core roadmap (Phase 8–9) stabilizes.

---

## Motivation

- Voice interaction is a natural complement to agent-deck's persona system
- Apple Silicon (M3/M4 Mac mini) is fast enough to run STT + LLM + TTS simultaneously without cloud dependency
- Cloud TTS (e.g. ElevenLabs at ~$22/mo for 100k chars) becomes expensive quickly with an active agent
- A native iOS companion app (planned separately) could add wake-word detection as a natural extension

---

## Proposed Architecture

```
[Browser Mic / iOS App] 
        │
        ▼
  STT (local Whisper or Parakeet)
        │
        ▼
  agent-deck Chat API (existing)
        │
        ▼
  TTS (local Kokoro or Piper)
        │
        ▼
  [Speaker / Headphones / Bone Conduction]
```

All components run locally on the host machine. The browser or iOS app handles mic capture; audio playback is either streamed back to the client or played directly on the host.

---

## Component Options

### Speech-to-Text (STT)

| Option | Notes |
|---|---|
| **whisper.cpp** | Rust bindings available (`whisper-rs`). Fast on Apple Silicon. Most mature local option. |
| **Parakeet (NVIDIA)** | High accuracy, fast. Python-based. Could run as a local sidecar. |
| **Web Speech API** | Built into browsers. Zero setup, zero latency overhead. Quality varies, no offline guarantee. |

**Recommendation:** `whisper.cpp` via `whisper-rs` for maximum integration with the Rust backend. Web Speech API as a lightweight fallback.

### Text-to-Speech (TTS)

| Option | Quality | Speed | Notes |
|---|---|---|---|
| **Kokoro TTS** | ⭐⭐⭐⭐⭐ | Very fast | Hot in local AI community. High quality, fast start. Python-based. |
| **Piper TTS** | ⭐⭐⭐ | Blazing | Lightweight C++ binary, near real-time. Good for minimal latency. |
| **Coqui TTS** | ⭐⭐⭐⭐ | Fast | More voices, voice cloning support. Runs well on Apple Silicon. |
| **macOS `say`** | ⭐⭐ | Instant | Zero setup. Native macOS. Quality is mediocre but always available. |
| **ElevenLabs (cloud)** | ⭐⭐⭐⭐⭐ | Fast | Best voice quality. ~$22/mo for 100k chars — expensive with frequent use. |

**Recommendation:** Kokoro TTS as primary local option. `say` as a zero-dependency fallback.

---

## Integration Points in agent-deck

- **New backend route:** `POST /voice/stt` — accepts audio blob, returns transcript
- **New backend route:** `GET /voice/tts?text=...` — returns audio stream
- **New SSE event (optional):** `VoiceStart` / `VoiceEnd` to signal voice mode in the UI
- **PWA UI:** Voice mode toggle button in the chat composer. Mic button replaces/supplements text input. Audio player for TTS response.
- **iOS App:** Wake-word detection via `SFSpeechRecognizer`, send transcript directly to chat API. Plays TTS response via `AVAudioPlayer`.

---

## Experimental / Future: Thought Interface (AlterEgo-style)

### Background

**AlterEgo** (MIT Media Lab, 2018 — Arnav Kapur) is a prototype neuromuscular interface that reads EMG signals from the jaw/submental area during internal subvocalization. You "think" words without speaking; the device reads the muscle signals, sends them to an AI, and delivers the response via bone conduction headphones. No audible speech in either direction.

The project spun out as a Boston startup also called **AlterEgo**, which has raised **$35.1M** as of the last known round.

**Current status (as of late 2025):** The company had a scheduled public demo in September 2025, but meaningful progress appears stalled. Their marketing leans heavily into "telepathic BCI" framing, which oversells what the tech actually is — it's **sEMG (surface electromyography)**, not direct brain reading. The signals come from subvocalization muscle activations, not neural signals. This is a meaningful distinction for both feasibility and expectations.

**Research:** The IUI 2018 paper is publicly available and describes the signal processing and ML pipeline.

---

### DIY Path

Consumer neuromuscular hardware has not matured enough to buy off-the-shelf AlterEgo-style input. However, a DIY proof-of-concept is viable with hobbyist BCI hardware:

**Hardware:**
- **OpenBCI Cyton board** — ~$500, 8 EMG/EEG channels, open-source firmware, good community support
- **Ag/AgCl electrodes** — standard wet electrodes; best signal quality for surface EMG placement around the jaw/submental area
- Placement targeting the same muscle groups as AlterEgo (masseter, digastric, submental)

**Signal pipeline:**
1. Raw sEMG → OpenBCI SDK or BrainFlow (Python) for signal capture
2. Feature extraction (RMS, frequency bands, time-domain features)
3. Classifier (small neural net or SVM) → word/token prediction
4. Output to agent-deck chat API

**Realistic outcome:** A rough proof-of-concept with a small, trained vocabulary (~10–30 words) is achievable. Full natural language is a research-grade effort and not realistic in a DIY context.

---

### Reverse Output Options (Agent → User, No Sound)

Complementary to the silent input path, several options exist for delivering agent responses without audible speech:

| Method | Notes |
|---|---|
| **Bone conduction (Shokz)** | Most practical. Transmits via skull vibration, leaves ears open. Off-the-shelf hardware. |
| **Parametric / ultrasonic audio** | Highly directional sound via ultrasonic transducer arrays (e.g. Holosonics). Heard only by the target person. Expensive and niche. |
| **Transcranial focused ultrasound (TUS)** | Research-grade neuromodulation — can induce sensory percepts directly. Far outside DIY territory currently. Worth watching as the field matures. |

**Practical recommendation for DIY:** Bone conduction (Shokz) is the obvious starting point — cheap, available today, and already used in the AlterEgo demos.

---

## Open Questions

- Does audio playback route through the browser client, or does the Rust backend play audio directly on the host machine?
- How does voice mode interact with streaming responses — does TTS wait for the full response, or does it chunk and speak sentence-by-sentence?
- Should voice mode have its own persona configuration (e.g. TTS voice selection per persona)?
- Wake word support — is this in scope, or push-to-talk only?
- For the DIY BCI path: what's the minimum viable vocabulary for the subvocalization classifier to be practically useful?

---

## Status & Next Steps

🧪 **Experimental — no active work planned.**

To move this forward, revisit after:
1. Phase 8 & 9 core roadmap is shipped
2. iOS companion app is underway (natural voice integration point)
3. A specific hardware setup (Mac mini M4) is in place for local model testing
4. *(BCI path only)* OpenBCI Cyton board acquired and initial sEMG signal capture explored
