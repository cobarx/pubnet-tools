package com.cobarx.pubnetchk

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.core.app.ActivityCompat
import androidx.lifecycle.viewmodel.compose.viewModel

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        setContent {
            MaterialTheme {
                val vm: AuditViewModel = viewModel()

                // Runtime location request. A denial with no rationale prompt
                // available means "don't ask again" (or policy) — route to the
                // app's settings page instead of silently doing nothing. Re-run
                // the audit afterward either way so the SSID row refreshes.
                val permissionLauncher = rememberLauncherForActivityResult(
                    ActivityResultContracts.RequestPermission(),
                ) { granted ->
                    if (!granted && !ActivityCompat.shouldShowRequestPermissionRationale(
                            this@MainActivity, Manifest.permission.ACCESS_FINE_LOCATION,
                        )
                    ) {
                        openSettings(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
                    }
                    vm.runAudit()
                }

                MainScreen(
                    onScan = { vm.runAudit() },
                    onFixWifiName = { status ->
                        when (status) {
                            WifiNameStatus.NO_PERMISSION ->
                                permissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION)
                            WifiNameStatus.LOCATION_OFF ->
                                openSettings(Settings.ACTION_LOCATION_SOURCE_SETTINGS)
                            else -> Unit
                        }
                    },
                    viewModel = vm,
                )
            }
        }
    }

    private fun openSettings(action: String) {
        val intent = Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (action == Settings.ACTION_APPLICATION_DETAILS_SETTINGS) {
            intent.data = Uri.fromParts("package", packageName, null)
        }
        runCatching { startActivity(intent) }
    }
}
