package com.cobarx.pubnetchk

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import uniffi.pubnetchk_android.AuditException
import uniffi.pubnetchk_android.runAuditJson

/**
 * Drives one audit: gather `HostSnapshot` from framework APIs → `runAuditJson`
 * (blocking, on `Dispatchers.IO`) → parse the report JSON → expose UI state.
 * All four checks run (topology, security, reliability, speed).
 */
class AuditViewModel(app: Application) : AndroidViewModel(app) {

    private val _state = MutableStateFlow<AuditUiState>(AuditUiState.Idle)
    val state: StateFlow<AuditUiState> = _state.asStateFlow()

    fun runAudit() {
        if (_state.value is AuditUiState.Running) return
        _state.value = AuditUiState.Running

        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                try {
                    val facts = NetworkFacts.collect(getApplication())
                    val snapshotJson = NetworkFacts.toJson(facts.snapshot)
                    val optionsJson = Json.encodeToString(AndroidOptions.serializer(), AndroidOptions())
                    Log.d(TAG, "snapshot: $snapshotJson")

                    val reportJson = runAuditJson(snapshotJson, optionsJson)
                    Log.d(TAG, "report: $reportJson")

                    val report = ReportJson.decoder.decodeFromString(Report.serializer(), reportJson)
                    AuditUiState.Done(report, facts.wifiName)
                } catch (e: AuditException) {
                    AuditUiState.Error("The audit engine rejected the request: ${e.message}")
                } catch (e: Exception) {
                    Log.e(TAG, "audit failed", e)
                    AuditUiState.Error(e.message ?: e.javaClass.simpleName)
                }
            }
            _state.value = result
        }
    }

    private companion object {
        const val TAG = "AuditViewModel"
    }
}

sealed interface AuditUiState {
    data object Idle : AuditUiState
    data object Running : AuditUiState
    data class Done(val report: Report, val wifiName: WifiNameStatus) : AuditUiState
    data class Error(val message: String) : AuditUiState
}
