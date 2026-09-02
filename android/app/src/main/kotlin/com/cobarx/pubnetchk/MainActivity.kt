package com.cobarx.pubnetchk

import android.Manifest
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.lifecycle.viewmodel.compose.viewModel

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        setContent {
            MaterialTheme {
                val vm: AuditViewModel = viewModel()

                // Reading the connected SSID needs ACCESS_FINE_LOCATION. If it is
                // denied the audit still runs — the SSID is just reported hidden.
                val permissionLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
                    ActivityResultContracts.RequestPermission(),
                ) { vm.runAudit() }

                MainScreen(
                    onScan = { permissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION) },
                    viewModel = vm,
                )
            }
        }
    }
}
