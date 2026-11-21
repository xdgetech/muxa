plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

dependencies {
    api(project(":transport"))
    api(libs.coroutines.core)
    implementation(libs.serialization.json)
    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
}

tasks.test {
    // `make test-android-v2` sets it to the built fake server; without it those tests skip.
    System.getenv("MUXA_TEST_V2_SERVER")?.let { systemProperty("muxa.v2server", it) }
}
