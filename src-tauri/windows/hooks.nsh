; Jusqu'à la version 0.1.28, l'appli avait un autre nom (programme, dossier et clés de registre
; ci-dessous). Les noms non ASCII sont écrits avec ${U+...} : ce fichier est lu sans BOM.
!define OLD_PRODUCTNAME "Aper${U+00E7}u"
!define OLD_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${OLD_PRODUCTNAME}"
!define OLD_MANUPRODUCTKEY "Software\Kilian Lef${U+00E8}vre\${OLD_PRODUCTNAME}"
!define APERCU_RAPIDE_TASK "Documents - relance apercu rapide"
!define ACTIVE_SETUP_KEY "Software\Microsoft\Active Setup\Installed Components\fr.marouteau.documents"

; Retire l'installation sous l'ancien nom trouvée sous ROOT (HKLM : pour tous, HKCU : pour
; l'utilisateur) avec son propre désinstallateur, sans effacer les données (signatures,
; réglages : elles restent, l'identifiant de l'appli n'a pas changé). Il ferme l'ancien
; programme, retire ses raccourcis, ses types de fichiers et sa tâche planifiée.
!macro DOCUMENTS_REMOVE_OLD ROOT
  ReadRegStr $R8 ${ROOT} "${OLD_UNINSTKEY}" "InstallLocation"
  ${If} $R8 != ""
    ; InstallLocation est écrit entre guillemets.
    StrCpy $R9 $R8 1
    ${If} $R9 == '"'
      StrCpy $R8 $R8 -1 1
    ${EndIf}
    ${If} ${FileExists} "$R8\uninstall.exe"
      ; _?= : le désinstallateur tourne sur place, et on attend vraiment sa fin.
      ExecWait '"$R8\uninstall.exe" /S _?=$R8'
      Delete "$R8\uninstall.exe"
      RMDir "$R8"
    ${EndIf}
    DeleteRegKey ${ROOT} "${OLD_UNINSTKEY}"
    DeleteRegKey ${ROOT} "${OLD_MANUPRODUCTKEY}"
  ${EndIf}
!macroend

; Avant d'installer une mise à jour : fermer toutes les copies de l'appli, y compris celles des
; autres sessions d'un serveur (aperçu rapide en arrière-plan, fenêtres laissées ouvertes).
; Sinon les fichiers sont verrouillés et la mise à jour échoue tant que tout le monde n'a pas
; quitté l'appli. Installé pour tous les utilisateurs, l'installateur est administrateur et
; peut fermer les copies des autres ; sinon seules les copies de l'utilisateur sont fermées.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM "${MAINBINARYNAME}.exe"'
  Pop $0
  nsExec::Exec 'taskkill /F /IM "apercu.exe"'
  Pop $0
  Sleep 800

  ; Passage de l'ancien nom à « Documents » : l'ancienne installation est remplacée. Celle pour
  ; tous seulement si cette installation-ci est aussi pour tous (il faut être administrateur).
  ${If} $MultiUser.InstallMode == "AllUsers"
    !insertmacro DOCUMENTS_REMOVE_OLD HKLM
  ${EndIf}
  !insertmacro DOCUMENTS_REMOVE_OLD HKCU
!macroend

; Après l'installation : relancer l'aperçu rapide dans chaque session ouverte. Une tâche
; planifiée pour le groupe « Utilisateurs » (S-1-5-32-545, quelle que soit la langue de
; Windows) démarre une copie dans la session de chaque utilisateur connecté ; elle remet à jour
; ses réglages, puis s'arrête aussitôt pour ceux qui n'ont pas coché l'aperçu rapide, ou chez
; qui il tourne déjà. La tâche n'a pas de déclencheur : elle reste inerte jusqu'à la prochaine
; mise à jour.
!macro NSIS_HOOK_POSTINSTALL
  ; Ancien nom du fichier des licences tierces (avant le passage en anglais).
  Delete "$INSTDIR\LICENCES-TIERS.txt"

  nsExec::Exec `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$a = New-ScheduledTaskAction -Execute '$INSTDIR\${MAINBINARYNAME}.exe' -Argument '--apercu-rapide'; $$p = New-ScheduledTaskPrincipal -GroupId 'S-1-5-32-545' -RunLevel Limited; $$s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances Parallel; Register-ScheduledTask -TaskName '${APERCU_RAPIDE_TASK}' -Action $$a -Principal $$p -Settings $$s -Force | Out-Null; Start-ScheduledTask -TaskName '${APERCU_RAPIDE_TASK}'"`
  Pop $0

  ; Installé pour tous : les utilisateurs qui ne sont pas connectés maintenant auront leurs
  ; réglages remis à jour à leur prochaine ouverture de session, une seule fois (« Active
  ; Setup » de Windows).
  ${If} $MultiUser.InstallMode == "AllUsers"
    WriteRegStr HKLM "${ACTIVE_SETUP_KEY}" "" "${PRODUCTNAME}"
    WriteRegStr HKLM "${ACTIVE_SETUP_KEY}" "StubPath" '"$INSTDIR\${MAINBINARYNAME}.exe" --reglages-utilisateur'
    WriteRegStr HKLM "${ACTIVE_SETUP_KEY}" "Version" "1"
    WriteRegDWORD HKLM "${ACTIVE_SETUP_KEY}" "IsInstalled" 1
  ${EndIf}

  ; Une icône par type de fichier dans l'Explorateur (feuille marquée PDF, JPG, PNG, WEBP, GIF),
  ; pour les types déclarés par l'installateur (noms tirés de tauri.conf.json).
  WriteRegStr SHCTX "Software\Classes\Document PDF\DefaultIcon" "" "$INSTDIR\types\pdf.ico"
  WriteRegStr SHCTX "Software\Classes\Image JPEG\DefaultIcon" "" "$INSTDIR\types\jpg.ico"
  WriteRegStr SHCTX "Software\Classes\Image PNG\DefaultIcon" "" "$INSTDIR\types\png.ico"
  WriteRegStr SHCTX "Software\Classes\Image WebP\DefaultIcon" "" "$INSTDIR\types\webp.ico"
  WriteRegStr SHCTX "Software\Classes\Image GIF\DefaultIcon" "" "$INSTDIR\types\gif.ico"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

; Désinstallation : fermer toutes les copies et retirer la tâche planifiée.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM "${MAINBINARYNAME}.exe"'
  Pop $0
  nsExec::Exec 'schtasks /Delete /TN "${APERCU_RAPIDE_TASK}" /F'
  Pop $0
  ; Pas pendant une mise à jour : la nouvelle version réécrit la même clé.
  ${If} $UpdateMode <> 1
    DeleteRegKey HKLM "${ACTIVE_SETUP_KEY}"
  ${EndIf}
  Sleep 800
!macroend
