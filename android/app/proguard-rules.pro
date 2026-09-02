# JNA + the UniFFI-generated bindings reach the cdylib by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.pubnetchk_android.** { *; }
