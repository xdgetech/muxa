plugins {
    alias(libs.plugins.kotlin.jvm)
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }

dependencies {
    api(project(":protocol"))
    api(project(":transport"))
    api(libs.coroutines.core)
    testImplementation(project(":session"))
    testImplementation(libs.junit)
}
