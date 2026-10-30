plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

dependencies {
    api(libs.coroutines.core)
    implementation(libs.serialization.json)
    implementation(libs.sshj)
    implementation(libs.bcprov)
    // sshj pulls an older bcpkix and bcutil; BouncyCastle's modules must share one version.
    constraints {
        implementation(libs.bcpkix)
        implementation(libs.bcutil)
    }
    // sshj can trace packets, which carry pane bytes; they never reach a log.
    runtimeOnly(libs.slf4j.nop)
    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
}
