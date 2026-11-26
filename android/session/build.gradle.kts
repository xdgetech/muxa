plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

// One copy of the bundled themes and their test cases, shared with Swift's MuxaSession (ADR 0091).
sourceSets {
    main { resources.srcDir("../../apple/MuxaKit/Sources/MuxaSession/Resources") }
    test { resources.srcDir("../../apple/MuxaKit/Tests/MuxaSessionTests/Fixtures") }
}

dependencies {
    api(project(":transport"))
    api(libs.coroutines.core)
    implementation(libs.serialization.json)
    testImplementation(libs.junit)
}
