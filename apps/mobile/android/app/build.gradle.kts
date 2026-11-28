plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "dev.paperwarden.paperwarden"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.paperwarden.paperwarden"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

// PDFium for the isolated render service (#59), fetched and verified by
// scripts/fetch_pdfium.py into third_party/pdfium/<platform>/lib. Only
// RenderService's process loads it. It ships as libpaperwarden_pdfium.so:
// every Android process already maps the system's own libpdfium.so, and a
// distinct name means only this build can be loaded. An ABI whose library was
// not fetched is left out; CI fetches all three and checks the APK carries each.
val pdfiumPlatforms = mapOf(
    "arm64-v8a" to "android-arm64",
    "armeabi-v7a" to "android-arm",
    "x86_64" to "android-x64",
)
val pdfiumJniLibs: File = layout.buildDirectory.dir("pdfium/jniLibs").get().asFile
val stagePdfium by tasks.registering(Sync::class) {
    pdfiumPlatforms.forEach { (abi, platform) ->
        from(rootProject.file("../../../third_party/pdfium/$platform/lib")) {
            include("libpdfium.so")
            rename("libpdfium[.]so", "libpaperwarden_pdfium.so")
            into(abi)
        }
    }
    into(pdfiumJniLibs)
}
android.sourceSets.getByName("main").jniLibs.srcDir(pdfiumJniLibs)
tasks.named("preBuild") { dependsOn(stagePdfium) }

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
