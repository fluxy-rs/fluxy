buildscript {
    // Standard repositories only. An earlier local copy listed Aliyun mirrors
    // first; they answer 502 from GitHub's runners, and Gradle treats a 5xx as
    // fatal instead of trying the next repository, so the Android build never
    // resolved. Add mirrors through ~/.gradle/init.gradle when they are needed.
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.3.1")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

tasks.register("clean").configure {
    delete("build")
}

