# Testing justRead on a phone or tablet

## Install

### Android
1. Open the latest **Mobile builds** run under the repository's *Actions* tab
   (branch `ccr-bc429417-7g7n6y`) and download the **justread-android-debug** artifact.
2. Unzip it; inside is an `.apk` (arm64 — fits practically every phone from the last ~8 years).
3. Copy it to the phone and open it. Android asks to allow installing from that app
   (Settings → Apps → Special access → *Install unknown apps*). Allow it once, then install.
   Alternative with a cable: `adb install app-*-debug.apk`.

It is a *debug* build: slower than release and signed with Android's debug key, so it installs
alongside nothing from a store and must be uninstalled before a future release build is installed.

### iPhone / iPad
Apple only installs signed apps, so iOS builds are made on a Mac with your Apple ID
(a free account works; the app then runs for 7 days before it needs reinstalling).

1. On the Mac: Xcode (App Store), Rust (`https://rustup.rs`), Node 22 and pnpm.
2. `rustup target add aarch64-apple-ios`
3. In the repository: `pnpm install`, then `pnpm tauri ios init`.
4. Xcode → Settings → Accounts: add your Apple ID.
5. Connect the iPhone, enable **Developer Mode** on it (Settings → Privacy & Security), then run
   `pnpm tauri ios dev` and pick the phone. On first launch, trust the developer in
   Settings → General → VPN & Device Management.

## What to try

| Area | Try | Expected |
|---|---|---|
| Import | *Import* button → pick an EPUB / PDF / CBZ / MOBI / FB2 | Book appears with a cover |
| Open with | Files app or Downloads → share/open a book with justRead | Book is imported and opens in the reader |
| Pages | Swipe left / right; tap the left / right third | Page turns; tap in the middle hides the bars |
| Layouts | Layout button → Single page / Two pages / Scroll | Same text stays on screen when switching |
| Rotation | Rotate the device while reading | Layout reflows, same place in the book |
| Selection | Long-press a word, drag the handles | Highlight menu appears; colors and notes work |
| Text | Aa → font size, margins, themes | Applies instantly, position kept |
| Comics/PDF | Swipe pages; pinch out, pan with one finger, pinch in | Zoom follows fingers; swipes pan while zoomed |
| Resume | Leave the app (home), come back; also force-close and reopen | Same position |
| Notch | Look at the top and bottom edges | Nothing hidden under the notch or home bar |

Known for now: DjVu and RAR comics (CBR)
are not supported.

## Reporting a problem
Please note: device and OS version, the book format, what you did, and what you saw
(a screenshot or screen recording helps most).
