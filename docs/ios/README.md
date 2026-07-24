# H.O.T-Jarvis on iOS — architecture & App Store readiness

Groundwork for shipping H.O.T-Jarvis to the App Store. Written on Windows, so
everything here is the platform-independent part: the plan, the config to apply,
the privacy manifest, the asset specs, and the exact Mac-only build/submit steps.
No build has run — see "What's blocked" below.

## What's blocked, honestly

- **Building/signing/submitting an iOS app requires macOS + Xcode.** There is no
  supported path from Windows to a `.ipa`, the Simulator, or Transporter. You
  have a Mac, so this is a "run these steps there" situation, not a wall.
- **App Store distribution requires the Apple Developer Program ($99/yr).** You
  aren't enrolled yet; enroll before the submit step. Everything up to that can
  be prepared now.
- **There is no Ollama on iOS.** The desktop app's "free, local, private
  inference" premise doesn't transfer directly — the phone needs a different
  answer for how it thinks (see the fork below). This is the one real product
  decision; the rest is execution.

## Architecture — Tauri v2 iOS target

Tauri v2 supports iOS, so we reuse most of what exists rather than rewrite:

| Layer | Reuse on iOS? | Notes |
|-------|---------------|-------|
| React UI (`src/`) | ✅ as-is | The whole HUD renders in the iOS WKWebView. Add safe-area insets + touch targets. |
| Rust core (`src-tauri/src/core/`) | ✅ mostly | `memory` (rusqlite), `skills` (Rhai), `eventlog`, `reflection`, `confidence`, `replay`, `reliability` all compile for iOS. Pure logic, no desktop deps. |
| `router` (Ollama/cloud) | ⚠️ rethink | Ollama can't run on iOS — see the fork. Cloud-tier calls work over the network. |
| `sysinfo` telemetry | ⚠️ degrade | iOS sandboxes system stats; CPU/RAM readouts won't be meaningful. Hide the telemetry strip on iOS or show app-scoped stats only. |
| Voice (`src/lib/voice.ts`) | ✅ swap engine | Use iOS native `AVSpeechSynthesizer` (TTS) and `SFSpeechRecognizer` (STT) via a small plugin — better than the web APIs and fully on-device. Needs usage-string permissions. |
| Notes / files | ✅ | Confined to the app's sandbox container (already how `NotesTool` works). |
| Data dir | ✅ | `app_data_dir()` resolves to the app's Documents/Application Support on iOS. |

## The inference fork (your decision — all three planned)

On iOS the phone can't run Ollama. Three ways to keep it useful; pick before build.

1. **Companion to your desktop (recommended, most on-brand).** The phone is a
   thin client to H.O.T-Jarvis running on your desktop, over your LAN (the
   desktop already has the router + Ollama). Inference stays **local, free,
   private**; the phone just talks to it. Cost: the desktop must be running and
   reachable. Work: add a small authenticated local HTTP endpoint to the desktop
   app + a device-pairing flow. Preserves the product's identity best.
2. **On-device model.** Embed a small LLM via MLC-LLM or llama.cpp (Metal) and
   run it on the phone. Fully self-contained and private, no desktop needed.
   Cost: large app binary (hundreds of MB), limited to small quantized models,
   heavier battery/RAM. Biggest engineering lift.
3. **Free cloud tiers.** The phone calls Groq / OpenRouter `:free` directly.
   Smallest and fastest to ship. Cost: no longer purely local/private on mobile
   — prompts leave the device, which **must** be disclosed in App Privacy and to
   the user. Rate-limited.

A pragmatic path: ship v1 as **(1) companion**, offer **(3) cloud** as an opt-in
fallback with a clear privacy notice, and treat **(2) on-device** as a later
premium option. Decide and this doc's build steps stay the same.

## App Store Review — the guidelines that actually bite here

- **2.1 App Completeness** — no crashes, no placeholder content, works on a real
  device at review. Ship a genuinely usable first run (don't require the desktop
  to be reachable to even open — degrade gracefully like the desktop already does
  with no provider).
- **4.2 Minimum Functionality** — a thin web wrapper gets rejected. This is fine:
  H.O.T-Jarvis is a real native-shell app with local storage, on-device voice,
  and the skill engine — well past "just a website."
- **2.5.2 No downloading/executing code** — ⚠️ **the one to get right.** The
  skill engine authors and runs **Rhai scripts** at runtime. Rhai is a sandboxed
  *interpreter* operating on user data (like spreadsheet formulas or Shortcuts),
  not downloaded native code, and it changes no app binary. Frame skills in the
  review notes as user-authored, sandboxed, interpreted content — not a code
  delivery mechanism. Do **not** fetch skills from a remote server on iOS; keep
  authoring local (the LLM writing a script the interpreter runs is analogous to
  a calculator app). If Review pushes back, gate skill *authoring* behind a
  clearly user-initiated action.
- **5.1.1 Data Collection & Storage / Privacy** — you must complete the App
  Privacy "nutrition labels." Local-only (option 1/2) → "Data Not Collected."
  Cloud (option 3) → disclose that prompt text is sent to a third-party model
  provider, linked or not to identity.
- **Encryption / export compliance** — set `ITSAppUsesNonExemptEncryption` in
  Info.plist. Using only HTTPS/OS crypto → exempt (`false`).
- **3.1.1 In-App Purchase** — the app is free with no digital goods sold; nothing
  to do. Keep it that way (the project is free-forever anyway).

## Privacy & permissions (Info.plist + manifest)

- `NSMicrophoneUsageDescription` — "H.O.T-Jarvis uses the microphone for
  voice conversations, processed on your device."
- `NSSpeechRecognitionUsageDescription` — "Speech is transcribed on your device
  to let you talk to the assistant."
- `NSLocalNetworkUsageDescription` (companion mode only) — "H.O.T-Jarvis connects
  to your desktop on your local network to run the assistant privately."
- `ITSAppUsesNonExemptEncryption` = `false` (HTTPS/OS-crypto only).
- **Privacy manifest** — ship `PrivacyInfo.xcprivacy` (template in this folder)
  declaring required-reason API usage (file timestamps, disk space, UserDefaults)
  and tracking = false.

## Assets (prepare on Windows; final export sizes on the Mac)

- **App icon**: ready — use `brand/app-icon-1024.png` (1024×1024, opaque, no
  rounded corners; iOS masks it). Xcode 14+ accepts the single 1024 and
  generates the set. Regenerate from `brand/icon-square.svg` with
  `node scripts/generate-icons.mjs` if the mark changes.
- **Launch screen**: a storyboard/plain color (near-black `#04070d` + the core
  mark). Keep it static; no launch animation.
- **Screenshots** (App Store Connect): 6.7" (1290×2796) and 6.5" (1242×2688)
  required; add 5.5" and iPad if universal. Capture from the Simulator on the Mac.
- **Support URL / marketing URL**: point at the landing page once deployed.

## Mac-only build & submit checklist (run when enrolled)

```bash
# on macOS with Xcode + rustup iOS targets
xcode-select --install
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

cd H.O.T-Jarvis
npm install
npm run tauri ios init            # generates the Xcode project under src-tauri/gen/apple

# apply the config snippets below, drop PrivacyInfo.xcprivacy into the Xcode project,
# set your Development Team (signing) in Xcode or tauri.conf.json

npm run tauri ios dev             # run on Simulator / device
npm run tauri ios build           # produces the archive / .ipa
# then: App Store Connect → new app record → upload via Xcode Organizer or Transporter
#       → TestFlight internal test → submit for review with the 2.5.2 note above
```

### tauri.conf.json — iOS block to add (kept out of the tested desktop config)

```json
"bundle": {
  "iOS": {
    "minimumSystemVersion": "15.0",
    "developmentTeam": "<YOUR_TEAM_ID>"
  }
}
```

(Applied on the Mac after `ios init`, so the green desktop build stays untouched
until it can be re-tested there.)

## What's prepared here vs. still needed

- ✅ Prepared (this PR): architecture, the inference fork with a recommendation,
  the Review-guideline analysis (incl. the 2.5.2 skill-engine consideration),
  privacy/permission strings, `PrivacyInfo.xcprivacy` template, asset specs, and
  the exact build/submit steps.
- ⏳ Needs your Mac: `tauri ios init`, signing, Simulator/device runs, the build.
- ⏳ Needs enrollment ($99): App Store Connect record, TestFlight, submission.
- ⏳ Needs your decision: the inference fork (1/2/3 above) before the router work.
