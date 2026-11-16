# Releases and in-app updates (Android)

Every push to the development branch builds an Android APK on GitHub Actions
(`.github/workflows/mobile.yml`). Once the signing key below is set up, each
build is:

1. numbered higher than the last (`0.2.<run number>`), so Android accepts it as an update;
2. signed with **one permanent justRead key**, so it installs over the existing app and keeps
   your library (Android only lets an app be updated by an APK signed with the same key);
3. published as a **GitHub Release** (`android-v0.2.N`) with `justRead.apk` and
   `justRead.apk.sha256`.

The app checks the latest release (Settings → Updates; automatic about once a day, can be
turned off), downloads the APK, checks it against the published SHA-256, and opens Android's
installer. Android asks you to confirm every time, and refuses an APK signed with any other key.

The update check is justRead's only network use: a request to `api.github.com` for the latest
release and, when you choose to update, the download from `github.com`. Nothing about you or
your books is sent. The repository must be **public** for the app to see its releases.

Without the key, the workflow keeps producing an unsigned-by-us debug APK as a build artifact
(the old behaviour, which needs an uninstall before each install).

## One-time setup: your signing key

You need a computer with Java (for `keytool`). On a Mac: install Java from
<https://adoptium.net> (Temurin), or `brew install openjdk`.

### 1. Create the key

In a terminal, in a folder you will keep (not inside the repository):

```sh
keytool -genkeypair -v -keystore justread.jks -alias justread \
  -keyalg RSA -keysize 4096 -validity 36500 \
  -dname "CN=justRead"
```

It asks for a **keystore password** twice; choose a long one and save it in your password
manager. (When it asks for a separate key password, press Enter to use the same one.)

**Back up `justread.jks` and the password.** If the key is lost, the next build can't update
the installed app: you would have to uninstall once (losing the library on that phone) and start
again with a new key.

### 2. Turn it into text for GitHub

```sh
base64 -i justread.jks | tr -d '\n' > justread.jks.txt      # macOS
base64 -w0 justread.jks > justread.jks.txt                   # Linux
```

### 3. Add four repository secrets

GitHub → justRead → **Settings → Secrets and variables → Actions → New repository secret**:

| Name | Value |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | the whole contents of `justread.jks.txt` |
| `ANDROID_KEYSTORE_PASSWORD` | your keystore password |
| `ANDROID_KEY_ALIAS` | `justread` |
| `ANDROID_KEY_PASSWORD` | the same password (unless you chose a different key password) |

Then delete `justread.jks.txt` (keep `justread.jks`).

### 4. Make the repository public

GitHub → justRead → **Settings → General → Danger Zone → Change visibility → Public**.
The app reads releases without any login, so they must be public.

### 5. The switch-over (once)

The next push builds the first signed release. Because the apps you installed so far were
signed with throwaway debug keys, **uninstall the old justRead one last time**, then install
`justRead.apk` from the newest release (GitHub → justRead → Releases). From then on, updates
install over the app from inside it: Settings → Updates, or the banner in the library.

## Notes

* Releases are published only from `main`; other branches
  still build (as a downloadable artifact) without publishing.
* The first time you update from inside the app, Android asks you to allow justRead to install
  apps; justRead opens that settings page for you, then tap Update again.
* Desktop builds don't update themselves yet; that would use Tauri's updater with its own
  signing key.
