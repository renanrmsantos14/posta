package br.com.renan.posta

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    SyncWorker.schedule(applicationContext)
  }

  override fun onStart() {
    super.onStart()
    SyncWorker.appVisible = true
  }

  override fun onStop() {
    SyncWorker.appVisible = false
    super.onStop()
  }
}
