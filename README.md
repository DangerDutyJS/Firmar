# Firmar Vecipp

Aplicación de escritorio para **firmar documentos con tu firma manuscrita** y detectar si alguien los modifica después. Funciona **100 % sin conexión**: tus firmas, claves y documentos nunca salen de tu equipo.

Desarrollada por **Vecipp Soluciones** (Colombia).

## Qué hace

- **Tu firma, guardada de forma segura**: dibújala o importa una foto; el fondo y los marcos se eliminan solos. Se guarda cifrada (AES-256-GCM) y la clave vive en el llavero de Windows.
- **Firma PDF, Word (`.docx`) e imágenes**: colócala con doble clic, muévela y ajústala desde cualquier lado. Puedes añadir tu nombre, la fecha o un texto debajo.
- **Detecta alteraciones**: cada documento lleva un sello criptográfico Ed25519 sobre su SHA-256. Al abrirlo en Firmar Vecipp verás si está íntegro, si fue alterado o si se le añadió contenido después.
- **Re-posiciona tu firma** sin acumular estampados: parte siempre del original guardado en la bóveda.
- **Firma varios documentos a la vez**, en la primera, la última o todas las páginas.
- **Historial** cifrado de lo que firmaste, con verificación instantánea.
- **Copia de seguridad** cifrada con contraseña (Argon2id) para cambiar de equipo sin perder nada.
- **«Firmar con Firmar Vecipp»** desde el menú contextual del Explorador de Windows.

## Alcance legal

Firmar Vecipp produce una **firma electrónica simple** (Ley 527 de 1999 y Decreto 2364 de 2012, Colombia). Es admisible como prueba, pero **no es una firma digital certificada**: no usa certificados de una entidad acreditada ante el ONAC ni verifica la identidad del firmante. Firmar Vecipp **detecta** los cambios en un documento firmado, pero no puede impedir que otro programa lo edite. Consulta los términos de uso y la política de privacidad dentro de la aplicación (**Acerca de**).

## Instalación

Descarga el instalador `.exe` desde [Releases](https://github.com/DangerDutyJS/Firmar/releases) y ejecútalo. Se instala para tu usuario, sin permisos de administrador. Requiere Windows 10 u 11 (WebView2 viene incluido en Windows 11 y el instalador lo descarga si falta).

## Desarrollo

Requisitos: [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs) estable y Visual Studio Build Tools (C++).

```bash
npm install
npm run tauri dev     # app en modo desarrollo
npm run tauri build   # instalador en src-tauri/target/release/bundle/nsis/
cd src-tauri && cargo test
```

## Arquitectura

| Capa | Ruta | Contenido |
|---|---|---|
| Dominio | `src-tauri/src/domain` | Modelos puros: perfiles, posiciones, metadatos |
| Criptografía | `src-tauri/src/crypto` | AES-256-GCM, Ed25519, SHA-256 |
| Contenedor | `src-tauri/src/container` | Sobre `X-Firmar-VaultPayload` (PDF/imagen al final del archivo, DOCX en el comentario del ZIP, otros en un `.firmar` aparte) |
| Motores | `pdf_engine`, `docx_engine`, `image_engine` | Estampado aplanado en PDF, imagen anclada en Word, PNG/JPEG |
| Almacenamiento | `src-tauri/src/storage` | Llavero del SO, bóveda cifrada, historial, copias de seguridad |
| Casos de uso | `src-tauri/src/services` | Firmar, verificar, re-posicionar, lotes |
| Frontend | `src/` | React 19, TypeScript, Tailwind 4, Zustand, pdf.js |

El documento de diseño original está en [`docs/ARQUITECTURA.md`](docs/ARQUITECTURA.md).

## Licencia

© 2026 Vecipp Soluciones. Todos los derechos reservados. Los componentes de terceros se distribuyen bajo sus propias licencias (ver **Acerca de · Licencias** en la aplicación).
