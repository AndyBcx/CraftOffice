plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// One APK per CraftOffice app: the workflow passes these (-PcoId=wordcraft -PcoName="CraftOffice Writer" …).
val coId = (findProperty("coId") as String?) ?: "wordcraft"
val coName = (findProperty("coName") as String?) ?: "CraftOffice"
val coPort = ((findProperty("coPort") as String?) ?: "47900").toInt()
val coVersion = (findProperty("coVersion") as String?) ?: "0.1"
val coCode = ((findProperty("coCode") as String?) ?: "1").toInt()

android {
    namespace = "app.craftoffice"
    compileSdk = 34

    defaultConfig {
        applicationId = "app.craftoffice." + coId.replace("-", "_")
        minSdk = 26
        targetSdk = 34
        versionCode = coCode
        versionName = coVersion
        resValue("string", "app_name", coName)
        buildConfigField("int", "APP_PORT", coPort.toString())
    }

    buildFeatures {
        buildConfig = true
    }
    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    androidResources {
        // The WebAssembly and font files are read as they are.
        noCompress += listOf("wasm", "woff2", "gz")
    }
    packaging {
        jniLibs {
            // The engine library is loaded from the extracted copy, as with any native library.
            useLegacyPackaging = true
        }
    }
}
