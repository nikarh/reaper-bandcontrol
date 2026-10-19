# Wire generated messages are accessed reflectively only through their ADAPTER fields.
-keep class dev.nikarh.bandcontrol.proto.** { *; }

# ML Kit wires its components reflectively; R8 full mode otherwise breaks GmsBarcodeScanning.getClient (NPE).
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_code_scanner.** { *; }
