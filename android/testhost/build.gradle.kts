plugins {
    alias(libs.plugins.kotlin.jvm)
    application
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

application { mainClass.set("tech.xdge.muxa.testhost.TestHostKt") }

dependencies {
    implementation(libs.sshd.core)
    // Ed25519 client keys: Apache MINA SSHD verifies them with BouncyCastle.
    implementation(libs.bcprov)
    runtimeOnly(libs.slf4j.nop)
}
