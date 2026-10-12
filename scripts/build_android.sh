#!/usr/bin/env bash
set -euo pipefail

if ! cargo apk --help >/dev/null 2>&1; then
    echo "Nedostaje cargo-apk. Instaliraj ga naredbom: cargo install cargo-apk" >&2
    exit 1
fi

android_sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [[ -z "$android_sdk" || ! -d "$android_sdk" ]]; then
    echo "Postavi ANDROID_SDK_ROOT na direktorij instaliranog Android SDK-a." >&2
    exit 1
fi

android_ndk="${ANDROID_NDK_HOME:-${NDK_HOME:-}}"
if [[ -z "$android_ndk" || ! -d "$android_ndk" ]]; then
    echo "Postavi ANDROID_NDK_HOME na direktorij instaliranog Android NDK-a." >&2
    exit 1
fi

export ANDROID_HOME="$android_sdk"
export ANDROID_SDK_ROOT="$android_sdk"
export ANDROID_NDK_HOME="$android_ndk"
export ANDROID_NDK="$android_ndk"

debug_keystore="target/android-debug.keystore"
if [[ ! -f "$debug_keystore" ]]; then
    keytool -genkeypair -noprompt \
        -keystore "$debug_keystore" \
        -storepass android \
        -alias androiddebugkey \
        -keypass android \
        -dname "CN=Android Debug,O=Citac,C=BA" \
        -keyalg RSA \
        -keysize 2048 \
        -validity 10000
fi

profile="debug"
for argument in "$@"; do
    if [[ "$argument" == "--release" || "$argument" == "-r" ]]; then
        profile="release"
    fi
done

cargo apk build --lib "$@"

android_jar="$android_sdk/platforms/android-35/android.jar"
build_tools="$android_sdk/build-tools/35.0.1"
java_classes="target/android-java-classes"
package_tree="target/android-package-tree"
compiled_resources="target/android-compiled-resources.zip"
unsigned_apk="target/$profile/apk/citac-saf-unsigned.apk"
aligned_apk="target/$profile/apk/citac-saf-aligned.apk"
final_apk="target/$profile/apk/citac.apk"

mkdir -p "$java_classes" "$package_tree/lib/arm64-v8a"
javac -source 8 -target 8 -bootclasspath "$android_jar" \
    -d "$java_classes" android/rs/citac/reader/CitacActivity.java
"$build_tools/d8" --lib "$android_jar" --output "$package_tree" \
    "$java_classes"/rs/citac/reader/CitacActivity*.class
install -m 755 "target/aarch64-linux-android/$profile/libcitac.so" \
    "$package_tree/lib/arm64-v8a/libcitac.so"
"$build_tools/aapt2" compile --dir android/res -o "$compiled_resources"
"$build_tools/aapt2" link -o "$unsigned_apk" \
    --manifest android/AndroidManifest.xml -I "$android_jar" \
    -R "$compiled_resources"
unsigned_absolute="$(realpath "$unsigned_apk")"
(
    cd "$package_tree"
    "$build_tools/aapt" add "$unsigned_absolute" classes.dex lib/arm64-v8a/libcitac.so
)
"$build_tools/zipalign" -f 4 "$unsigned_apk" "$aligned_apk"
"$build_tools/apksigner" sign --ks "$debug_keystore" --ks-pass pass:android \
    --out "$final_apk" "$aligned_apk"
"$build_tools/apksigner" verify --verbose "$final_apk"
