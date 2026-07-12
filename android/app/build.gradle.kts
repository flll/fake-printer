plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "jp.flll.fakeprinter"
    compileSdk = 34

    defaultConfig {
        applicationId = "jp.flll.fakeprinter"
        minSdk = 24
        targetSdk = 34
        versionCode = 2
        versionName = "0.2.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core:1.13.1") // FileProvider
    implementation("org.jmdns:jmdns:3.5.9")     // mDNS/AirPrint advertisement
    testImplementation("junit:junit:4.13.2")
}
