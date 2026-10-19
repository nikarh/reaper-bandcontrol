import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.wire)
    alias(libs.plugins.android.lint)
}

lint {
    warningsAsErrors = true
    abortOnError = true
    // TrulyRandom only concerns Android 4.3 and older, the app's minSdk is 28.
    disable += setOf("GradleDependency", "NewerVersionAvailable", "TrulyRandom")
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
        allWarningsAsErrors = true
    }
}

wire {
    sourcePath {
        srcDir(rootProject.file("../proto"))
    }
    kotlin {}
}

dependencies {
    api(libs.wire.runtime)
    api(libs.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(libs.json)
    testImplementation(libs.coroutines.test)
}

tasks.test {
    systemProperty("bandcontrol.vectors", rootProject.file("../proto/test-vectors.json").path)
    // Simulator for the cross-language tests, built by `cargo build -p bandctl` (see `mise run android:test`).
    systemProperty("bandcontrol.bandctl", rootProject.file("../target/debug/bandctl").path)
    inputs.file(rootProject.file("../proto/test-vectors.json"))
    testLogging {
        events("failed")
        exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    }
}
