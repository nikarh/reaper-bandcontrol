import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// One version and one project address for everything: `[workspace.package]` in the root Cargo.toml.
fun workspacePackage(key: String): String =
    rootProject
        .file("../Cargo.toml")
        .readLines()
        .dropWhile { it.trim() != "[workspace.package]" }
        .first { it.trim().startsWith(key) }
        .substringAfter('"')
        .substringBefore('"')

val appVersion: String = workspacePackage("version")
val repository: String = workspacePackage("repository")
val appVersionCode: Int = appVersion.split('.').map(String::toInt).let { (major, minor, patch) -> major * 10_000 + minor * 100 + patch }

// Release signing comes from the environment (the release workflow); local builds fall back to the debug key.
val releaseKeystore: String? = System.getenv("ANDROID_KEYSTORE")?.takeIf { it.isNotEmpty() }

android {
    namespace = "dev.nikarh.bandcontrol.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.nikarh.bandcontrol"
        minSdk = 28
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "REPOSITORY", "\"$repository\"")
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "release" else "debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    lint {
        warningsAsErrors = true
        abortOnError = true
        checkDependencies = true
        // Version freshness is managed by hand in libs.versions.toml.
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "OldTargetApi")
    }

    packaging {
        resources.excludes += setOf("META-INF/AL2.0", "META-INF/LGPL2.1")
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
        allWarningsAsErrors = true
    }
}

dependencies {
    implementation(project(":core"))
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.camera.camera2)
    implementation(libs.camera.lifecycle)
    implementation(libs.camera.view)
    implementation(libs.compose.icons)
    implementation(libs.reorderable)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.core.ktx)
    implementation(libs.coroutines.android)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.androidx.test.ext)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.compose.ui.test)
    androidTestImplementation(libs.uiautomator)
}
