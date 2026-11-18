## Fluxy release rules.
##
## R8 shrinks the archive from 24.7 MB to 9.6 MB, but it also renamed the ML Kit
## barcode internals and the scanner broke: "Attempt to read from field
## 'al1 cg1.a' on a null object reference". The bundled model loader reaches
## those classes by name, so names have to survive; shrinking still removes the
## rest of the dependency graph.

-dontobfuscate

# Barcode scanning: the bundled model is loaded by reflection, and the plugin
# points at it from Kotlin that R8 cannot see through.
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_barcode_bundled.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_common.** { *; }
-keep class app.tauri.barcodescanner.** { *; }

# Tauri reaches plugin classes through the generated registry; keeping the
# framework intact costs little and avoids another silent breakage.
-keep class app.tauri.** { *; }
