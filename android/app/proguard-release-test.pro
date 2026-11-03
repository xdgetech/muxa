# Added to the app only for `make test-android-release`. The test APK leaves out the
# libraries the app already has and calls the app's copies, so these must survive R8.
# Nothing here touches muxa, sshj, JediTerm or BouncyCastle: the test still checks those.
-keep class androidx.tracing.** { *; }
-keep class kotlin.** { *; }
