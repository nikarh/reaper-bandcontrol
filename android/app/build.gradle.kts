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
// Written out in Cargo.toml for F-Droid, which reads it with a regular expression; it must follow the version.
val appVersionCode: Int =
    rootProject
        .file("../Cargo.toml")
        .readLines()
        .first { it.trim().startsWith("version-code") }
        .substringAfter('=')
        .trim()
        .toInt()
        .also { code ->
            val expected = appVersion.split('.').map(String::toInt).let { (major, minor, patch) -> major * 10_000 + minor * 100 + patch }
            require(code == expected) { "version-code $code in Cargo.toml doesn't match version $appVersion (expected $expected)" }
        }

// Release builds are signed with the release key when it is in the environment, unsigned with -Punsigned (CI: the
// Publish workflow signs them once, with apksigner, which keeps them verifiable for F-Droid's reproducible builds),
// and with the debug key otherwise, so local release builds install.
val releaseKeystore: String? = System.getenv("ANDROID_KEYSTORE")?.takeIf { it.isNotEmpty() }
val unsigned: Boolean = providers.gradleProperty("unsigned").isPresent

android {
    namespace = "dev.nikarh.bandcontrol.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.nikarh.bandcontrol"
        minSdk = 28
        targetSdk = 36
        versionCode = appVersionCode
        // BANDCONTROL_VERSION_SUFFIX: builds of main, e.g. 0.1.2-dev+abc1234 (the `latest` pre-release).
        versionName = appVersion + System.getenv("BANDCONTROL_VERSION_SUFFIX").orEmpty()
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
            signingConfig =
                when {
                    releaseKeystore != null -> signingConfigs.getByName("release")
                    unsigned -> null
                    else -> signingConfigs.getByName("debug")
                }
        }
    }

    // No Google-encrypted dependency list in the APK signing block: F-Droid rejects it, and it gets in the way of
    // reproducible builds.
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
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
