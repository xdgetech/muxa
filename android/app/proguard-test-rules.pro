# The instrumented-test APK of the release build (make test-android-release). R8 maps its
# references onto the minified app. These classes come from the device or the compiler.
-dontwarn androidx.window.extensions.**
-dontwarn javax.lang.model.element.Modifier
