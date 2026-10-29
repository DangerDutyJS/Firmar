; Ganchos del instalador NSIS de Tauri.
;
; Añade «Firmar con Firmar» al menú contextual del Explorador para PDF, Word e
; imágenes. Se usa SystemFileAssociations, que agrega la opción sin cambiar el
; programa predeterminado de cada tipo de archivo. Todo en HKCU: no requiere
; permisos de administrador y se elimina al desinstalar.

!macro FIRMAR_ADD_VERB EXT
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\Firmar" "" "Firmar con Firmar Vecipp"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\Firmar" "Icon" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\Firmar\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro FIRMAR_REMOVE_VERB EXT
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\${EXT}\shell\Firmar"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro FIRMAR_ADD_VERB ".pdf"
  !insertmacro FIRMAR_ADD_VERB ".docx"
  !insertmacro FIRMAR_ADD_VERB ".png"
  !insertmacro FIRMAR_ADD_VERB ".jpg"
  !insertmacro FIRMAR_ADD_VERB ".jpeg"
  ; Avisa al Explorador para que muestre la opción sin reiniciar.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro FIRMAR_REMOVE_VERB ".pdf"
  !insertmacro FIRMAR_REMOVE_VERB ".docx"
  !insertmacro FIRMAR_REMOVE_VERB ".png"
  !insertmacro FIRMAR_REMOVE_VERB ".jpg"
  !insertmacro FIRMAR_REMOVE_VERB ".jpeg"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
