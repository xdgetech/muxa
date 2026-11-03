# Release builds: R8 removes unused code, renames and optimizes (ADR 0030).
# Keep only what is created from a class name. `make test-android-release` connects
# over SSH with the minified app; run it after changing these rules or a dependency.

# BouncyCastleProvider registers algorithms by class name: it loads each `$Mappings`
# class, and the JCA creates each SPI class with reflection. Names and constructors only.
-keep class org.bouncycastle.jcajce.provider.** { <init>(...); }
-keep class org.bouncycastle.pqc.jcajce.provider.** { <init>(...); }
-keep class org.bouncycastle.jce.provider.** { <init>(...); }

# BouncyCastle's EST package (certificate enrollment over TLS) has a trust-everything
# X509TrustManager that Play's security scan rejects. Nothing above keeps it, so R8
# drops it; muxa never uses TLS. ADR 0014.

# Java SE classes Android lacks, used only on paths muxa never takes: JediTerm's AWT
# resize, BouncyCastle's LDAP and DNS lookups, sshj's Kerberos login.
-dontwarn java.awt.**
-dontwarn javax.naming.**
-dontwarn javax.security.auth.login.**
-dontwarn org.ietf.jgss.**
