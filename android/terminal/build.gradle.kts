plugins {
    alias(libs.plugins.kotlin.jvm)
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

dependencies {
    api(libs.jediterm.core)
    // JediTerm logs escape sequences, which are pane bytes; they never reach a log.
    runtimeOnly(libs.slf4j.nop)
    testImplementation(project(":protocol"))
    testImplementation(libs.serialization.json)
    testImplementation(libs.junit)
}

tasks.test {
    systemProperty("muxa.fixtures", rootDir.resolve("../protocol/fixtures").absolutePath)
}
