# Release builds: R8 removes unused code and changes nothing else (ADR 0014).
# BouncyCastle, sshj and JediTerm load classes by name, and the demo never runs SSH, so
# nothing is renamed or rewritten, and whole libraries are kept.
-dontobfuscate
-dontoptimize

-keep class tech.xdge.muxa.** { *; }
-keep class net.schmizz.** { *; }
-keep class com.hierynomus.** { *; }
-keep class com.jediterm.** { *; }
-keep class org.slf4j.** { *; }

# All of BouncyCastle except EST (certificate enrollment over TLS). Its trust-everything
# X509TrustManager is what Play's security scan rejects; muxa never uses TLS.
-keep class !org.bouncycastle.est.**,org.bouncycastle.** { *; }

# Java SE classes Android lacks, used only on paths muxa never takes: JediTerm's AWT
# resize, BouncyCastle's LDAP and DNS lookups, sshj's Kerberos login.
-dontwarn java.awt.**
-dontwarn javax.naming.**
-dontwarn javax.security.auth.login.**
-dontwarn org.ietf.jgss.**
