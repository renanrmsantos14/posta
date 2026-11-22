; Migração Posta -> RenanMail no Windows. O instalador do Tauri guarda a pasta, a entrada em
; "Programas e Recursos" e os atalhos pelo nome do produto; com o nome novo, a atualização
; instalaria ao lado da antiga. Aqui a antiga sai (sem apagar os dados: ficam na pasta do
; identificador br.com.renan.posta, a mesma do RenanMail) e o "iniciar com o Windows" é levado junto.
!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Posta" "UninstallString"
  ${If} $R0 != ""
    ExecWait '$R0 /S'
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Posta"
  ${EndIf}
  Delete "$SMPROGRAMS\Posta.lnk"
  Delete "$DESKTOP\Posta.lnk"
  ReadRegStr $R1 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Posta"
  ${If} $R1 != ""
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Posta"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "RenanMail" '"$INSTDIR\${MAINBINARYNAME}.exe" --minimized'
  ${EndIf}
!macroend
