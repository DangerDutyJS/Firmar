# PROYECTO: FIRMAR (Desktop Cryptographic Signing Suite)

Actúa como un Principal Rust & TypeScript Systems Architect. Estamos desarrollando **Firmar**, una aplicación de escritorio 100% offline, ultra rápida y escalable construida con **Tauri v2 (Rust backend) + React 19 / TypeScript / Tailwind CSS (Frontend)**.

## OBJETIVO CORE

Permitir al usuario guardar su firma digital preestablecida en una bóveda local cifrada y estamparla en documentos (PDF, Word `.docx`, imágenes y cualquier archivo binario).

- **Regla de Oro de Seguridad:** Una vez firmado el documento, ningún programa externo puede editar ni remover la firma sin corromper la validez criptográfica y visual del archivo. Sin embargo, cuando el archivo se abre nuevamente dentro de **Firmar** en la máquina autorizada, nuestro motor en Rust detecta el contenedor oculto cifrado (`X-Firmar-VaultPayload`), lo desencripta usando la llave del Keyring del Sistema Operativo y permite al propietario gestionar, auditar o re-posicionar su firma.

## ARQUITECTURA (HEXAGONAL / CLEAN ARCHITECTURE)

1. **Backend (`src-tauri/src/`):**
   - `domain/`: Modelos puros (`SignatureProfile`, `SignedDocument`, `FirmarMetadataPayload`).
   - `crypto/`: Motor criptográfico usando `ring`, `aes-gcm` (AES-256-GCM), `ed25519-dalek` (firmas asimétricas de curva elíptica) y `sha2`.
   - `storage/`: Integración nativa con el Keyring del SO (`keyring` crate para Windows Credential Manager / Linux Secret Service) y base de datos local para la bóveda de trazos.
   - `pdf_engine/`: Manipulación y aplanado de PDF con `lopdf`, inyectando el diccionario propietario `/FirmarPayload` cifrado en el catálogo raíz del PDF.
   - `commands/`: Exposición de comandos Tauri IPC tipados hacia el frontend.
2. **Frontend (`src/`):**
   - `features/vault/`: Lienzo de dibujo vectorial y carga de firma con remoción de fondo automática en cliente.
   - `features/signer/`: Visor de documentos con `pdfjs-dist`, capa interactiva Drag & Drop para ubicar la firma en coordenadas exactas `(x, y, page, scale)`.
   - `store/`: Estado global con Zustand.

## REGLAS DE GENERACIÓN DE CÓDIGO

- Escribe código de grado de producción, sin `todo!()` vacíos ni placeholders.
- Maneja todos los errores en Rust con `thiserror` y `Result<T, FirmarError>` serializables para Tauri.
- Prioriza ejecución 100% local sin llamadas de red externas.

---

> **Nota (implementación):** este es el documento de diseño original. Dos puntos se resolvieron de otra forma:
>
> - La «Regla de Oro» no es alcanzable tal como está escrita: ningún formato puede *impedir* que otro programa edite un archivo. Firmar **detecta** cualquier cambio (Ed25519 sobre el SHA-256 exacto del contenido firmado) y lo informa al abrir el documento.
> - El producto genera una **firma electrónica simple**, no una «firma digital» en el sentido legal (Ley 527 de 1999), porque no usa certificados de una entidad acreditada ante el ONAC.
> - No se usa `ring`: `aes-gcm` + `getrandom` cubren lo mismo. La bóveda es un conjunto de archivos cifrados con AES-256-GCM en lugar de una base de datos.
