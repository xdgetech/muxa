plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "tech.xdge.muxa.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "tech.xdge.muxa"
        // Keystore keys bound to an unlocked device, and ImageDecoder (HEIC), start at 28.
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets {
        // The Reader's page and its vendored scripts are shared with the iOS app. Both
        // folders are roots: the page loads the scripts from beside it, as on iOS.
        getByName("main").assets.directories += listOf("../../apple/Muxa/Muxa/Reader", "../../apple/Muxa/Muxa/Reader/vendor")
    }

    androidResources {
        // The shared Reader folder also holds the iOS app's Swift files, and vendor/ is its own root.
        ignoreAssetsPatterns += listOf("*.swift", "<dir>vendor")
    }

    packaging {
        resources {
            // BouncyCastle and sshj bring license and module files of their own.
            excludes += setOf("META-INF/versions/**", "META-INF/*.md", "META-INF/DEPENDENCIES", "META-INF/LICENSE*", "META-INF/NOTICE*")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions { unitTests.isReturnDefaultValues = true }
}

dependencies {
    implementation(project(":session"))
    implementation(libs.bcprov)
    implementation(libs.serialization.json)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.icons)
    implementation(libs.adaptive)
    implementation(libs.adaptive.layout)
    implementation(libs.adaptive.navigation)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.process)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.window)
    implementation(libs.webkit)
    implementation(libs.core.ktx)
    implementation(libs.markdown.m3)

    testImplementation(libs.junit)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    // Compose's test library brings Espresso 3.5, which calls an InputManager method API 37 removed.
    androidTestImplementation(libs.espresso.core)
    androidTestImplementation(libs.window.testing)
    debugImplementation(libs.compose.ui.test.manifest)
    // The in-app demo bridge: Try the Demo, and the UI tests.
    implementation(project(":demo"))
}
