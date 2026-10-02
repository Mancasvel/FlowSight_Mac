// Explicit application copy. User-authored task names, messages and activity are never translated.
export const spanish = Object.fromEntries(`
Planning runs on this device. Review and confirm to add the blocks to your linked calendar, or to FlowSight if none is connected.|La planificación se ejecuta en este dispositivo. Revisa y confirma para añadir los bloques a tu calendario vinculado, o a FlowSight si no hay ninguno conectado.
Draft ready. Review the times and estimates before adding the blocks to {calendar}.|Borrador listo. Revisa las horas y estimaciones antes de añadir los bloques a {calendar}.
{count} block added to {calendar}.|{count} bloque añadido a {calendar}.
{count} blocks added to {calendar}.|{count} bloques añadidos a {calendar}.
Saving the reviewed blocks to your calendar…|Guardando los bloques revisados en tu calendario…
Reviewed session awaiting calendar saving|Sesión revisada pendiente de guardar en el calendario
Finish saving your session|Termina de guardar tu sesión
Retry saving reviewed blocks|Reintentar el guardado de los bloques revisados
Stop saving remaining blocks|Detener el guardado de los bloques restantes
Stop saving the remaining blocks? Events already sent will stay in your linked calendar. An interrupted request may also have created an event there. Check your calendar before planning again.|¿Detener el guardado de los bloques restantes? Los eventos ya enviados se conservarán en tu calendario vinculado. Una petición interrumpida también puede haber creado un evento allí. Revisa tu calendario antes de planificar otra sesión.
Remaining save stopped. Existing linked-calendar events have been kept.|Se ha detenido el guardado restante. Se han conservado los eventos del calendario vinculado.
Your local calendar overlaps this reviewed session. Resolve the overlap before retrying its save.|Tu calendario local coincide con esta sesión revisada. Resuelve la coincidencia antes de reintentar su guardado.
The reviewed session is not fully saved to {calendar}: {saved} of {total} blocks confirmed. Retry to finish saving these same blocks.|La sesión revisada no está completamente guardada en {calendar}: {saved} de {total} bloques confirmados. Reintenta para terminar de guardar estos mismos bloques.
Finish saving your reviewed session before suggesting another one.|Termina de guardar tu sesión revisada antes de solicitar otra propuesta.
Finish saving your reviewed session before confirming another one.|Termina de guardar tu sesión revisada antes de confirmar otra.
Your linked calendar changed. Suggest a fresh session before confirming.|Tu calendario vinculado ha cambiado. Solicita una nueva propuesta antes de confirmar.
Your linked calendar now overlaps this proposal. Adjust the session and suggest it again.|Ahora hay eventos en tu calendario vinculado que coinciden con esta propuesta. Ajusta la sesión y solicita otra propuesta.
Reconnect the calendar used for this reviewed session before retrying its save.|Vuelve a conectar el calendario de esta sesión revisada antes de reintentar su guardado.
Your linked calendar overlaps this reviewed session. Resolve the overlap in your calendar, then retry saving.|Tu calendario vinculado coincide con esta sesión revisada. Resuelve la coincidencia en tu calendario y reintenta el guardado.
Understand your own Workflows|Comprende tu forma de trabajar
Understand your own workflows.|Comprende tu forma de trabajar.
More settings below|Más opciones abajo
Continue|Continuar
Skip setup|Omitir configuración
Would you like to help improve FlowSight?|¿Quieres ayudar a mejorar FlowSight?
If you agree, FlowSight will send pseudonymous usage data including:|Si aceptas, FlowSight enviará datos de uso seudónimos, entre ellos:
Daily time of use|Tiempo de uso diario
The main core activity you perform during the week|La actividad principal que realizas durante la semana
This uses a random installation ID, not your account or email. It is still treated as personal data, retained for up to 35 days, and deleted when you withdraw. FlowSight works fully without sharing it.|Se utiliza un identificador de instalación aleatorio, sin tu cuenta ni correo. Se trata como dato personal, se conserva hasta 35 días y se elimina al retirar tu consentimiento. FlowSight funciona por completo sin compartirlo.
No thanks|No, gracias
Yes, help improve|Sí, quiero ayudar
Keep FlowSight close at hand?|¿Quieres tener FlowSight a mano?
Start local tracking after sign-in|Iniciar el seguimiento local al iniciar sesión
Requires a separate acknowledgement of what local tracking observes. Leave this off to start tracking manually.|Requiere aceptar por separado qué observa el seguimiento local. Déjalo desactivado para iniciar el seguimiento manualmente.
Let local AI suggest focus reminders|Permitir que la IA local sugiera recordatorios de concentración
Only while tracking is on. Local Qwen sees aggregate signals and may choose not to notify. Reminders use generic wording unless you separately enable app and task names in Profile.|Solo mientras el seguimiento está activo. Qwen local ve señales agregadas y puede decidir no avisar. Los recordatorios son genéricos salvo que actives por separado los nombres de aplicaciones y tareas en Ajustes.
Not now|Ahora no
Open at sign-in|Abrir al iniciar sesión
Before tracking starts|Antes de iniciar el seguimiento
FlowSight will observe the active application, selected interface elements, and periodic images of the active window while tracking is on.|Mientras el seguimiento esté activo, FlowSight observará la aplicación activa, algunos elementos de su interfaz e imágenes periódicas de la ventana activa.
Screenshots are analyzed by AI on this device and are never saved.|La IA analiza las capturas en este dispositivo y nunca se guardan.
Only short activity summaries, categories, app names, and durations are kept locally.|Solo se conservan localmente resúmenes breves de actividad, categorías, nombres de aplicaciones y duraciones.
Window titles are not stored by default. Local history is deleted after 30 days by default.|Los títulos de las ventanas no se guardan por defecto. El historial local se elimina a los 30 días por defecto.
Nothing is synced to FlowSight or an AI provider unless you separately enable it.|No se sincroniza nada con FlowSight ni con un proveedor de IA salvo que lo actives por separado.
Opening at sign-in, automatic tracking, and focus reminders are separate choices in your profile.|Abrir al iniciar sesión, el seguimiento automático y los recordatorios de concentración son opciones independientes en Ajustes.
Pause or stop tracking at any time. Do not use FlowSight to monitor another person without a valid lawful basis and the required workplace notice.|Puedes pausar o detener el seguimiento en cualquier momento. No uses FlowSight para supervisar a otra persona sin una base jurídica válida y la información laboral exigida.
Read remaining details|Leer los detalles restantes
Cancel|Cancelar
I understand — start|Lo entiendo: iniciar
Privacy notice|Aviso de privacidad
Local tracking|Seguimiento local
Optional cloud purposes|Usos opcionales en la nube
Activity sync:|Sincronización de actividad:
sends locally generated aggregate summaries, categories, durations, ticket references and team/account IDs to FlowSight's Supabase service.|envía resúmenes agregados generados localmente, categorías, duraciones, referencias a incidencias e identificadores de equipo y cuenta al servicio Supabase de FlowSight.
Cloud AI:|IA en la nube:
if you enable it and explicitly use a cloud AI feature, selected work context may be processed by FlowSight and its AI providers, including Azure OpenAI or OpenRouter. The proactive local agent does not use cloud AI.|si la activas y utilizas expresamente una función de IA en la nube, FlowSight y sus proveedores, incluidos Azure OpenAI u OpenRouter, podrán procesar el contexto de trabajo seleccionado. El agente local proactivo no usa IA en la nube.
Product analytics:|Analíticas de producto:
sends a random installation ID, seven daily usage totals and the week's primary category. No account ID or email is included.|envía un identificador de instalación aleatorio, siete totales diarios de uso y la categoría principal de la semana. No incluye el identificador de cuenta ni el correo.
Jira and Linear receive only data needed for actions you request under their own privacy terms.|Jira y Linear reciben solo los datos necesarios para las acciones que solicitas, según sus propias condiciones de privacidad.
Your choices and rights|Tus opciones y derechos
Optional sharing is off by default and can be withdrawn here without affecting local tracking. You can export your data, erase local data, or delete your cloud account. You may also request access, correction, restriction, objection, portability, or complain to your supervisory authority by contacting manuel@flowsight.site.|El envío opcional está desactivado por defecto y puedes retirarlo aquí sin afectar al seguimiento local. Puedes exportar tus datos, borrar los datos locales o eliminar tu cuenta en la nube. También puedes solicitar acceso, rectificación, limitación, oposición o portabilidad, o reclamar ante tu autoridad de control, escribiendo a manuel@flowsight.site.
Retention and transfers|Conservación y transferencias
Local summaries use your selected period (30 days by default). Cloud activity and insight data is configured for 90 days, pseudonymous analytics for 35 days, and voluntary feedback for 12 months. Vendors may process data outside the EEA under an adequacy decision or Standard Contractual Clauses. Account and statutory billing records may be retained only where legally required.|Los resúmenes locales usan el plazo que elijas (30 días por defecto). La actividad y los informes en la nube se configuran para 90 días, las analíticas seudónimas para 35 días y los comentarios voluntarios para 12 meses. Los proveedores pueden procesar datos fuera del EEE mediante una decisión de adecuación o cláusulas contractuales tipo. Los registros de cuenta y facturación solo se conservarán cuando lo exija la ley.
Before production, the controller must publish the completed Article 13 notice at a stable, accessible location. The current technical draft and implementation checklist are maintained in|Antes de producción, el responsable debe publicar el aviso completo del artículo 13 en un lugar estable y accesible. El borrador técnico y la lista de implementación se encuentran en
and|y
Close|Cerrar
Sample activity · stored locally|Actividad de ejemplo · guardada localmente
Analysis|Análisis
Writing|Escritura
Research|Investigación
Daily goal streak|Racha del objetivo diario
Keep it up — 3 days to a 7-day streak!|¡Sigue así! Faltan 3 días para una racha de 7 días.
Work report ready|Informe de trabajo listo
Local AI generated your weekly status from tracked activity.|La IA local generó tu informe semanal a partir de la actividad registrada.
Welcome to FlowSight.|Te damos la bienvenida a FlowSight.
Email|Correo electrónico
Password|Contraseña
Sign in|Iniciar sesión
or continue with|o continuar con
Continue with Google|Continuar con Google
Trouble connecting? Use manual code|¿Problemas al conectar? Usa un código manual
Access Token (JWT)|Token de acceso (JWT)
Login with code|Iniciar sesión con código
Back|Atrás
Signing in creates a cloud account and sends your account details to Supabase. Activity sharing remains off until you enable it.|Al iniciar sesión se crea una cuenta en la nube y se envían sus datos a Supabase. El envío de actividad permanece desactivado hasta que lo actives.
Read the Privacy Notice|Leer el aviso de privacidad
Continue in Free (Local) mode|Continuar en modo gratuito (local)
Report preview|Vista previa del informe
Today|Hoy
What's next?|¿Qué viene ahora?
Start a block. Activity is measured only while tracking runs.|Inicia un bloque. La actividad solo se mide mientras el seguimiento está activo.
Tracking session|Sesión de seguimiento
Ready|Listo
Open|Abrir
Calendar time elapsed|Tiempo transcurrido del evento
Daily goal|Objetivo diario
No goal|Sin objetivo
Streak|Racha
0 days|0 días
Start tracking|Iniciar seguimiento
Stop|Detener
Let’s plan today’s session|Planifiquemos la sesión de hoy
What would you like to get done?|¿Qué quieres hacer?
Start today|Inicio hoy
Finish today|Fin hoy
Add estimates and fixed commitments if you know them. The local agent uses your tasks, saved preferences and recorded task time to suggest blocks. Include meetings from other calendars here.|Añade estimaciones y compromisos fijos si los conoces. El agente local usa tus tareas, preferencias guardadas y tiempo registrado para sugerir bloques. Incluye aquí las reuniones de otros calendarios.
Suggest my session|Sugerir mi sesión
Your plan stays on this device. Review and confirm before it is added to the FlowSight calendar.|Tu plan permanece en este dispositivo. Revísalo y confírmalo antes de añadirlo al calendario de FlowSight.
Your proposed session|Tu sesión propuesta
What would you change?|¿Qué cambiarías?
Adjust proposal|Ajustar propuesta
Confirm and add blocks|Confirmar y añadir bloques
Discard draft|Descartar borrador
Your remaining blocks today|Tus bloques restantes de hoy
Task details|Detalles de la tarea
Task description|Descripción de la tarea
Linked task|Tarea vinculada
Select Task...|Seleccionar tarea…
General / No Ticket|General / Sin incidencia
Study|Estudio
Recorded today|Registrado hoy
Timer mode|Modo del temporizador
Normal|Normal
Pomodoro|Pomodoro
Tracked today|Tiempo de seguimiento hoy
Focus interval|Intervalo de concentración
Ready for the next interval|Listo para el siguiente intervalo
Long break|Descanso largo
Intervals|Intervalos
Work (min)|Trabajo (min)
Break (min)|Descanso (min)
Long break (min)|Descanso largo (min)
Long break after four focus intervals. Breaks pause tracking; start the next interval when ready.|Descanso largo tras cuatro intervalos de concentración. El seguimiento se pausa durante los descansos; inicia el siguiente intervalo cuando estés listo.
Time for a break. Tracking is paused.|Es hora de descansar. El seguimiento está en pausa.
Break finished. Start the next focus interval when you are ready.|El descanso ha terminado. Inicia el siguiente intervalo de concentración cuando estés listo.
Could not pause tracking. Pause it before taking a break.|No se pudo pausar el seguimiento. Páusalo antes de descansar.
Taking a break|En descanso
Start focus interval|Iniciar intervalo de concentración
End break|Terminar descanso
Start the next focus interval when you are ready.|Inicia el siguiente intervalo de concentración cuando estés listo.
{p0} completed intervals · {p1} tracked today|{p0} intervalos completados · {p1} de seguimiento hoy
{p0} in sustained blocks · {p1} analyzed|{p0} en bloques sostenidos · {p1} analizados
Updates after each local analysis.|Se actualiza tras cada análisis local.
Task|Tarea
Sync time to Jira|Sincronizar tiempo con Jira
Off|Desactivado
Loading summary...|Cargando resumen…
What's next for your work?|¿Qué sigue en tu trabajo?
Ask about focus, activity, and planning, grounded in your tracked work.|Pregunta sobre concentración, actividad y planificación a partir de tu trabajo registrado.
Thinking…|Pensando…
Focus this week|Concentración esta semana
Distractions|Distracciones
Activate Coach|Activar Coach
Unlock personalized guidance from your tracked activity.|Obtén orientación personalizada a partir de tu actividad registrada.
Activate cloud|Activar la nube
Your space|Tu espacio
You|Tú
Make FlowSight work the way you do.|Adapta FlowSight a tu forma de trabajar.
Free (Local)|Gratis (local)
Checking Vision Model...|Comprobando el modelo de visión…
Guest|Invitado
Free local mode|Modo local gratuito
Activate license|Activar licencia
Enter your license code to activate the features included in your plan.|Introduce el código de licencia para activar las funciones de tu plan.
License code|Código de licencia
Activate|Activar
Work preferences|Preferencias de trabajo
Not set yet.|Sin configurar.
Edit work preferences|Editar preferencias de trabajo
Calendar companion|Asistente de calendario
Cloud|Nube
No calendar connected|Sin calendario conectado
Manage calendars|Gestionar calendarios
Your current event appears in Today. Choose whether to add a short evidence-based recap when it ends.|El evento actual aparece en Hoy. Elige si quieres añadir un breve resumen basado en la actividad registrada al terminar.
Google Calendar|Calendario de Google
Microsoft Calendar|Calendario de Microsoft
Not connected|Sin conectar
Connect|Conectar
Disconnect|Desconectar
Add a mini work report to events I organize|Añadir un breve informe de trabajo a los eventos que organizo
Includes events with guests. Guests may see the recap, and their calendar service may notify them. FlowSight never includes window titles or private activity descriptions. No report is posted when recorded activity is too brief to be useful or events overlap.|Incluye eventos con invitados. Pueden ver el resumen y su servicio de calendario puede avisarles. FlowSight nunca incluye títulos de ventanas ni descripciones privadas. No se publica ningún informe si la actividad registrada es demasiado breve o los eventos se solapan.
Local automations|Automatizaciones locales
Connect your browser|Conecta tu navegador
Optional site and tab actions in Arc or Chrome|Acciones opcionales en sitios y pestañas de Arc o Chrome
Checking…|Comprobando…
Install Browser Controls|Instalar Browser Controls
Open the official Chrome Web Store page and add the extension to Arc or Chrome. Its pairing page opens automatically after installation.|Abre la página oficial de Chrome Web Store y añade la extensión a Arc o Chrome. Su página de vinculación se abre automáticamente tras instalarla.
Open Chrome Web Store|Abrir Chrome Web Store
Open Edge Add-ons|Abrir complementos de Edge
Pair on this computer|Vincular en este equipo
Copy the key below, paste it into the extension's pairing page, then choose|Copia la clave, pégala en la página de vinculación de la extensión y elige
Save and connect|Guardar y conectar
Local port|Puerto local
Pairing key|Clave de vinculación
Copy pairing key|Copiar clave de vinculación
Check the connection|Comprobar la conexión
Return to FlowSight after saving. Browser actions become available when the status above says Connected.|Vuelve a FlowSight después de guardar. Las acciones del navegador estarán disponibles cuando el estado indique Conectado.
Check connection|Comprobar conexión
Tools and saved preferences|Herramientas y preferencias guardadas
Review action|Revisar acción
Confirm action|Confirmar acción
Available actions|Acciones disponibles
What FlowSight remembers|Lo que recuerda FlowSight
No saved preferences.|Sin preferencias guardadas.
Open FlowSight at computer sign-in|Abrir FlowSight al iniciar sesión en el equipo
Start local tracking at sign-in|Iniciar el seguimiento local al iniciar sesión
Requires launch at sign-in and your local tracking acknowledgement.|Requiere abrir al iniciar sesión y aceptar el seguimiento local.
Let local Qwen suggest focus reminders|Permitir que Qwen local sugiera recordatorios de concentración
Only while tracking is on. Qwen may propose a reminder after at least 2 minutes of non-work browsing or 12 switches across 3 apps within 10 minutes. It sees aggregate counts, not names or screen content, and can choose not to notify. FlowSight enforces a 30-minute cooldown.|Solo mientras el seguimiento está activo. Qwen puede proponer un recordatorio tras 2 minutos de navegación ajena al trabajo o 12 cambios entre 3 aplicaciones en 10 minutos. Ve recuentos agregados, sin nombres ni contenido de pantalla, y puede decidir no avisar. FlowSight exige 30 minutos entre avisos.
Add the app or site and my selected task when known|Incluir la aplicación o el sitio y la tarea seleccionada cuando se conozcan
Weekly work report|Informe semanal de trabajo
Automatically save the same seven-day PDF as Work report. FlowSight must be open, including in the system tray.|Guardar automáticamente el mismo PDF de siete días que Informe de trabajo. FlowSight debe estar abierto, también en la bandeja del sistema.
Save a report every week|Guardar un informe cada semana
Day|Día
Monday|Lunes
Tuesday|Martes
Wednesday|Miércoles
Thursday|Jueves
Friday|Viernes
Saturday|Sábado
Sunday|Domingo
Local time|Hora local
Save in folder|Guardar en carpeta
Choose a folder|Elegir una carpeta
Choose|Elegir
On the selected day, FlowSight runs at the set time or when you next open it that day. It uses your computer's local time.|El día elegido, FlowSight genera el informe a la hora indicada o al abrirlo después ese mismo día. Usa la hora local del equipo.
Save schedule|Guardar programación
Open last report folder|Abrir carpeta del último informe
Optional sharing is off by default. Each purpose can be changed independently.|El envío opcional está desactivado por defecto. Cada uso se puede cambiar por separado.
Sync aggregate activity to FlowSight Cloud|Sincronizar actividad agregada con FlowSight Cloud
Off — activity stays on this device.|Desactivado: la actividad permanece en este dispositivo.
Allow optional cloud AI processing|Permitir el procesamiento opcional con IA en la nube
Off — cloud AI features cannot receive activity.|Desactivado: las funciones de IA en la nube no pueden recibir actividad.
Share pseudonymous product analytics|Compartir analíticas de producto seudónimas
Local history|Historial local
Store full window titles in local history|Guardar títulos completos de ventanas en el historial local
Off by default. Turning it off also removes titles already stored.|Desactivado por defecto. Al desactivarlo también se eliminan los títulos ya guardados.
Never monitor these applications|No supervisar nunca estas aplicaciones
Exact application names, one per line. Matching apps are skipped entirely.|Nombres exactos de las aplicaciones, uno por línea. Las aplicaciones coincidentes se omiten por completo.
Delete local activity automatically after|Eliminar la actividad local automáticamente tras
7 days|7 días
30 days|30 días
90 days|90 días
1 year|1 año
Your data|Tus datos
Read privacy notice|Leer aviso de privacidad
Export my data|Exportar mis datos
Erase local data|Borrar datos locales
Delete cloud account|Eliminar cuenta en la nube
Feedback|Comentarios
Send voluntary feedback to FlowSight|Enviar comentarios voluntarios a FlowSight
Send feedback|Enviar comentarios
Do not include confidential or sensitive personal data. Your message and app version are retained for up to 12 months.|No incluyas datos confidenciales ni datos personales sensibles. Tu mensaje y la versión de la aplicación se conservan hasta 12 meses.
App updates|Actualizaciones
Version —|Versión —
Check for updates|Buscar actualizaciones
Connect your AI|Conecta tu IA
Generate a FlowSight work report in any desktop AI client that supports local MCP (STDIO). No extra runtime or FlowSight cloud account is needed.|Genera un informe de FlowSight en cualquier cliente de IA de escritorio compatible con MCP local (STDIO). No hace falta otro entorno de ejecución ni una cuenta en la nube de FlowSight.
Show connection details|Mostrar detalles de conexión
MCP command|Comando MCP
Copy command path|Copiar ruta del comando
Reports exclude activity descriptions and ticket IDs by default. A cloud AI client may receive the report data it requests.|Los informes excluyen por defecto las descripciones de actividad y los identificadores de incidencias. Un cliente de IA en la nube puede recibir los datos del informe que solicite.
Integrations|Integraciones
Task Provider (paid plans)|Proveedor de tareas (planes de pago)
Link Jira|Vincular Jira
Link Linear|Vincular Linear
Jira and Linear linking require an eligible paid plan.|Para vincular Jira y Linear se requiere un plan de pago compatible.
Team Code (from PM)|Código de equipo (del responsable)
Join|Unirse
Active Team|Equipo activo
No team selected|Sin equipo seleccionado
Insights|Informes
Settings|Ajustes
Retry download|Reintentar descarga
Later|Más tarde
Switch to dark mode|Cambiar al modo oscuro
Switch to light mode|Cambiar al modo claro
Dark mode|Modo oscuro
Minimize|Minimizar
Maximize|Maximizar
Restore|Restaurar
Hide to system tray|Ocultar en la bandeja del sistema
Set up FlowSight|Configurar FlowSight
Tracking and privacy details|Detalles de seguimiento y privacidad
Illustrative local activity preview using sample data|Vista previa ilustrativa de actividad local con datos de ejemplo
Your account password|Contraseña de tu cuenta
Paste Supabase access token…|Pega el token de acceso de Supabase…
Close report|Cerrar informe
Open current calendar event|Abrir el evento actual del calendario
Calendar event elapsed time|Tiempo transcurrido del evento del calendario
Stop tracking and shut down server|Detener seguimiento y apagar servidor
Finish the proposal (about 90 min), review feedback, then prepare tomorrow’s meeting…|Terminar la propuesta (unos 90 min), revisar comentarios y preparar la reunión de mañana…
Session proposal|Propuesta de sesión
Move the review earlier, allow 15 minutes for a break…|Adelantar la revisión, dejar 15 minutos de descanso…
Local session calendar|Calendario local de sesiones
What are you working on?|¿En qué estás trabajando?
Coach conversation|Conversación con Coach
Ask about your focus, activity, or week…|Pregunta sobre tu concentración, actividad o semana…
Message Coach|Mensaje para Coach
Send message|Enviar mensaje
How focused was I this week?|¿Cómo me he concentrado esta semana?
What should I tackle next?|¿Qué debería abordar ahora?
Where am I losing time to distractions?|¿Dónde pierdo tiempo por distracciones?
Sign out of cloud features|Cerrar sesión de las funciones en la nube
Calendar connections|Conexiones de calendario
Pairing key, hidden for your privacy|Clave de vinculación, oculta por privacidad
One application name per line|Un nombre de aplicación por línea
Tell us what you think about FlowSight…|Cuéntanos qué opinas de FlowSight…
MCP executable path|Ruta del ejecutable MCP
Enter team code...|Introduce el código de equipo…
Primary navigation|Navegación principal
Coach, Pro|Coach, Pro
Could not find the MCP command:|No se encontró el comando MCP:
Clipboard unavailable|Portapapeles no disponible
MCP command path copied|Ruta del comando MCP copiada
Select and copy the command path manually|Selecciona y copia la ruta del comando manualmente
Add context to this event (optional)|Añadir contexto a este evento (opcional)
What are you doing during this event?|¿Qué haces durante este evento?
Connected on this device|Conectado en este dispositivo
Connection not configured in this build|Conexión sin configurar en esta versión
Waiting for browser authorization…|Esperando autorización en el navegador…
Requires an eligible FlowSight Cloud plan. The one-time local purchase does not include Calendar.|Requiere un plan de FlowSight Cloud compatible. La compra local única no incluye Calendario.
Calendar requires an eligible active FlowSight Cloud plan with integrations. The one-time local purchase does not include Cloud integrations.|Calendario requiere un plan activo de FlowSight Cloud con integraciones. La compra local única no incluye integraciones en la nube.
Calendar is temporarily unavailable. Check the connection in You.|Calendario no está disponible temporalmente. Comprueba la conexión en Ajustes.
Calendar connected · your current event will appear here automatically.|Calendario conectado · el evento actual aparecerá aquí automáticamente.
Cloud plan required · connect after activation|Requiere un plan en la nube · conecta después de activarlo
mini reports on|resúmenes activados
mini reports off|resúmenes desactivados
Connect Google or Microsoft Calendar|Conectar Calendario de Google o Microsoft
Automatic recap is on for events you organize while FlowSight is open.|El resumen automático está activado para los eventos que organizas mientras FlowSight está abierto.
Automatic recap is off. Your calendar stays read-only.|El resumen automático está desactivado. El calendario sigue en modo de solo lectura.
Calendar requires an eligible FlowSight Cloud plan with integrations.|Calendario requiere un plan de FlowSight Cloud compatible con integraciones.
Calendar connected|Calendario conectado
Calendar authorization timed out. Try connecting again.|La autorización del calendario ha caducado. Intenta conectar de nuevo.
Calendar disconnected from this device|Calendario desconectado de este dispositivo
Calendar mini reports enabled|Resúmenes del calendario activados
Calendar mini reports disabled|Resúmenes del calendario desactivados
This feature requires an Individual or Team license.|Esta función requiere una licencia Individual o Team.
Enter your license code (FS-XXXX-XXXX).|Introduce el código de licencia (FS-XXXX-XXXX).
Activating license...|Activando licencia…
License could not be activated. Check your code and try again.|No se pudo activar la licencia. Comprueba el código e inténtalo de nuevo.
License activated|Licencia activada
Could not activate license.|No se pudo activar la licencia.
Cloud account|Cuenta en la nube
Team|Equipo
Licensed|Con licencia
Activate cloud features|Activar funciones en la nube
Sign in, then activate your license in Profile.|Inicia sesión y activa la licencia en Ajustes.
Unexpected app error|Error inesperado de la aplicación
User|Usuario
Cloud features active|Funciones en la nube activas
Enter your email and password.|Introduce tu correo y contraseña.
Cloud features activated|Funciones en la nube activadas
Signed in — activate your license in Profile|Sesión iniciada: activa la licencia en Ajustes
Could not save daily goal|No se pudo guardar el objetivo diario
In flow.|En marcha.
On hold.|En pausa.
Your work is being recorded locally. Pause or stop whenever you need.|Tu trabajo se registra localmente. Puedes pausar o detener cuando lo necesites.
The clock is paused. Resume when you are ready.|El reloj está en pausa. Reanuda cuando quieras.
Pause tracking|Pausar seguimiento
Resume tracking|Reanudar seguimiento
Live|En directo
Paused|En pausa
On|Activado
Tracking paused — time kept for today|Seguimiento en pausa: el tiempo de hoy se conserva
Could not pause tracking:|No se pudo pausar el seguimiento:
Tracking stopped|Seguimiento detenido
Tracking stopped, but the local AI server could not shut down:|Seguimiento detenido, pero el servidor local de IA no pudo apagarse:
Could not stop tracking:|No se pudo detener el seguimiento:
Focus block resumed|Bloque de concentración reanudado
Tracking resumed|Seguimiento reanudado
Error resuming:|Error al reanudar:
Linked|Vinculado
Active team|Equipo activo
Jira and Linear require an Individual or Team license.|Jira y Linear requieren una licencia Individual o Team.
Login option unavailable.|Opción de inicio de sesión no disponible.
Login timeout - please try again|El inicio de sesión ha caducado: inténtalo de nuevo
Login failed:|Error al iniciar sesión:
Back to Free (Local) mode|De vuelta al modo gratuito (local)
Logout failed:|Error al cerrar sesión:
Connecting...|Conectando…
Jira linked successfully!|Jira vinculado correctamente
Linear linked successfully!|Linear vinculado correctamente
Link timeout - try again|La vinculación ha caducado: inténtalo de nuevo
Link failed:|Error al vincular:
Joining a team requires a Team license.|Para unirte a un equipo necesitas una licencia Team.
Please enter a team code|Introduce un código de equipo
Joining...|Uniéndose…
Validating code...|Validando código…
Successfully joined the team!|Te has unido al equipo
Failed to join:|No se pudo unir al equipo:
Team code|Código de equipo
Sign in and activate a cloud plan to talk with your AI Coach.|Inicia sesión y activa un plan en la nube para hablar con tu Coach de IA.
Activate an eligible cloud license to use your AI Coach.|Activa una licencia compatible en la nube para usar tu Coach de IA.
Coach is not in this plan|Coach no está incluido en este plan
Your current plan does not include cloud AI chat.|Tu plan actual no incluye chat con IA en la nube.
View plan|Ver plan
Cloud AI is off|La IA en la nube está desactivada
Enable cloud AI in You → Privacy & data before sharing messages or work context.|Activa la IA en la nube en Ajustes → Privacidad y datos antes de compartir mensajes o contexto de trabajo.
Review privacy settings|Revisar ajustes de privacidad
limit reached|límite alcanzado
Could not load your Coach conversation.|No se pudo cargar la conversación con Coach.
Version unknown|Versión desconocida
You are on the latest version.|Tienes la última versión.
Update server unreachable or installer missing. Try again later or download from GitHub.|No se pudo contactar con el servidor de actualizaciones o falta el instalador. Inténtalo más tarde o descarga desde GitHub.
Could not check for updates. Try again later.|No se pudieron buscar actualizaciones. Inténtalo más tarde.
Update available|Actualización disponible
Install the new version now, or keep working and do it later.|Instala ahora la nueva versión o sigue trabajando e instálala más tarde.
Update details|Detalles de la actualización
FlowSight will restart after installation.|FlowSight se reiniciará después de la instalación.
Starting download…|Iniciando descarga…
Update now|Actualizar ahora
Update download|Descarga de la actualización
Downloading…|Descargando…
Installing… the app will restart.|Instalando… la aplicación se reiniciará.
Installer not found. Try again later or download it from GitHub Releases.|No se encontró el instalador. Inténtalo más tarde o descárgalo desde GitHub Releases.
The update could not be installed. Try again or use the official GitHub release.|No se pudo instalar la actualización. Inténtalo de nuevo o usa la versión oficial de GitHub.
The update is installed, but FlowSight could not restart. Quit it from the tray, then open it again.|La actualización está instalada, pero FlowSight no pudo reiniciarse. Sal desde la bandeja del sistema y vuelve a abrirlo.
Restart FlowSight to finish the update.|Reinicia FlowSight para completar la actualización.
Downloading local AI model…|Descargando el modelo local de IA…
Verifying model…|Verificando modelo…
Local model ready|Modelo local listo
Starting the local AI engine…|Iniciando el motor local de IA…
Preparing the local model|Preparando el modelo local
Initializing...|Inicializando…
Starting local AI…|Iniciando IA local…
local AI|IA local
Verifying Server...|Verificando servidor…
Waiting for health check — first start or slow disk may take several minutes…|Esperando la comprobación: el primer inicio o un disco lento puede tardar varios minutos…
Verifying Server…|Verificando servidor…
your FlowSight app data folder (server.log)|la carpeta de datos de FlowSight (server.log)
No health response yet — restarting on CPU only (slower but more compatible).|Sin respuesta todavía: reiniciando solo con CPU (más lento, pero más compatible).
FlowSight server.log under your account's local application data folder|server.log de FlowSight en la carpeta local de datos de tu cuenta
Local AI is not ready|La IA local no está lista
FlowSight could not prepare local AI. Please try again, or restart the app.|FlowSight no pudo preparar la IA local. Inténtalo de nuevo o reinicia la aplicación.
Local Server Ready|Servidor local listo
Server Offline|Servidor desconectado
Software development|Desarrollo de software
Design & UX|Diseño y experiencia de usuario
Product / project management|Gestión de producto y proyectos
Data & analytics|Datos y analíticas
Learning / student work|Aprendizaje y estudio
Other knowledge work|Otro trabajo intelectual
Writing & content|Escritura y contenido
Research & education|Investigación y educación
Sales & customer work|Ventas y atención al cliente
Operations & administration|Operaciones y administración
Finance & legal|Finanzas y asuntos legales
Writing & shipping code|Escribir y entregar código
Debugging & fixing issues|Depurar y corregir errores
Code review & collaboration|Revisión de código y colaboración
Meetings & calls|Reuniones y llamadas
Planning & documentation|Planificación y documentación
Research & learning|Investigación y aprendizaje
Design & prototyping|Diseño y prototipado
Support & operations|Soporte y operaciones
Analysis & spreadsheets|Análisis y hojas de cálculo
Writing & presentations|Escritura y presentaciones
Email & communication|Correo y comunicación
Sales & client work|Ventas y clientes
Admin & finance|Administración y finanzas
Spend less time on distracting apps|Dedicar menos tiempo a aplicaciones que distraen
Understand how I spend my time and make better decisions|Entender cómo uso mi tiempo y tomar mejores decisiones
Improve my attention and deep focus|Mejorar mi atención y concentración sostenida
Improve work-life balance|Mejorar el equilibrio entre trabajo y vida personal
Track progress on goals and tickets|Seguir el progreso de objetivos e incidencias
Feel good about my productivity|Sentirme bien con mi productividad
Complete the setup wizard to personalize reports and greetings.|Completa la configuración para personalizar informes y saludos.
Sign in to cloud features to set up your work preferences.|Inicia sesión en las funciones en la nube para configurar tus preferencias de trabajo.
Write · 60 min|Escribir · 60 min
Break|Descanso
Review|Revisar
Example plan · your tasks set the schedule|Plan de ejemplo · tus tareas definen el horario
Example session: write, break, then review|Sesión de ejemplo: escribir, descansar y revisar
Recorded evidence → a report saved on your computer|Actividad registrada → un informe guardado en tu equipo
Recorded work becomes a local report|El trabajo registrado se convierte en un informe local
Keep your focus|Mantén la concentración
A moment to refocus|Un momento para concentrarte
You've switched apps several times. Choose “Write proposal” for the next few minutes.|Has cambiado de aplicación varias veces. Elige «Escribir propuesta» para los próximos minutos.
You've switched among several apps. Could you choose one task for the next few minutes?|Has cambiado entre varias aplicaciones. ¿Puedes elegir una tarea para los próximos minutos?
Example|Ejemplo
Example FlowSight desktop notification|Ejemplo de notificación de escritorio de FlowSight
Example with a fictional task. Your selected task can appear when context is enabled.|Ejemplo con una tarea ficticia. La tarea seleccionada puede aparecer si activas el contexto.
Example desktop notification. Task and activity details stay out by default.|Ejemplo de notificación. Los detalles de tareas y actividad no se incluyen por defecto.
Finish setup|Terminar configuración
Make room for your work|Haz espacio para tu trabajo
Start tracking when you choose. FlowSight helps you review your time and work patterns on this computer.|Inicia el seguimiento cuando quieras. FlowSight te ayuda a revisar tu tiempo y hábitos de trabajo en este equipo.
What should we call you?|¿Cómo te llamas?
Optional|Opcional
Your first name|Tu nombre
Daily tracking goal|Objetivo diario de seguimiento
A reference for your day. You can pause or stop tracking at any time.|Una referencia para tu día. Puedes pausar o detener el seguimiento cuando quieras.
Add work context (optional)|Añadir contexto de trabajo (opcional)
Your role or area of work|Tu función o área de trabajo
For example, research or software development|Por ejemplo, investigación o desarrollo de software
A plan you can change|Un plan que puedes cambiar
Tell the local agent what you want to do and when you are available. It suggests time blocks using your estimates and the context you have saved.|Dile al agente local qué quieres hacer y cuándo estás disponible. Sugerirá bloques usando tus estimaciones y el contexto guardado.
Ask for changes, then confirm. Your blocks are added to the FlowSight calendar only after you approve them.|Pide cambios y después confirma. Los bloques se añaden al calendario de FlowSight solo cuando los apruebas.
Open the session planner after setup|Abrir el planificador al terminar la configuración
Planning is optional. You can also start tracking directly from Today.|Planificar es opcional. También puedes iniciar el seguimiento desde Hoy.
Find your way back to focus|Vuelve a concentrarte
While tracking, FlowSight can show a local reminder when your activity drifts from the task you selected.|Durante el seguimiento, FlowSight puede mostrar un recordatorio local si tu actividad se aleja de la tarea seleccionada.
Enable focus reminders|Activar recordatorios de concentración
Include recent work context in reminders|Incluir contexto reciente de trabajo en los recordatorios
Context may include task or activity details in a desktop notification. Both choices can be changed in Settings.|El contexto puede incluir detalles de tareas o actividad en una notificación. Ambas opciones se pueden cambiar en Ajustes.
See what your week contained|Revisa tu semana
Generate a work report from recorded activity in Insights. You can also save a weekly PDF automatically while FlowSight is running.|Genera un informe a partir de la actividad registrada en Informes. También puedes guardar un PDF semanal automáticamente mientras FlowSight está abierto.
Save a weekly report automatically|Guardar un informe semanal automáticamente
Save on this computer|Guardar en este equipo
Choose report folder|Elegir carpeta de informes
The report is an interpretation of recorded evidence. It is not a productivity score.|El informe interpreta la actividad registrada. No es una puntuación de productividad.
Choose a folder to enable automatic reports.|Elige una carpeta para activar los informes automáticos.
Choose where to save weekly reports|Elige dónde guardar los informes semanales
Could not choose a folder:|No se pudo elegir una carpeta:
Bring your calendar into focus|Conecta tu calendario
Optional for cloud plans. See your current event and, if you choose, add a short work recap after it ends.|Opcional para planes en la nube. Consulta el evento actual y, si quieres, añade un breve resumen de trabajo al terminar.
Calendar requires an eligible FlowSight Cloud plan with integrations. The one-time local purchase does not include it. Sign in and activate your Cloud plan in You after setup.|Calendario requiere un plan de FlowSight Cloud con integraciones. La compra local única no lo incluye. Inicia sesión y activa tu plan en Ajustes tras la configuración.
Add a mini report to events I organize|Añadir un breve informe a los eventos que organizo
If guests are invited, they may see the recap or receive a calendar update. No window titles or private descriptions are included. You can change this later in Settings.|Los invitados pueden ver el resumen o recibir una actualización del calendario. No incluye títulos de ventanas ni descripciones privadas. Puedes cambiarlo después en Ajustes.
Setup updated|Configuración actualizada
Saving…|Guardando…
Could not save this step:|No se pudo guardar este paso:
Could not finish setup:|No se pudo terminar la configuración:
Back to top|Volver arriba
Install FlowSight to enable launch at sign-in. Focus reminders can be tested here.|Instala FlowSight para abrirlo al iniciar sesión. Aquí puedes probar los recordatorios.
FlowSight is registered to open at your next computer sign-in.|FlowSight está configurado para abrirse en el próximo inicio de sesión.
FlowSight will only open when you launch it yourself.|FlowSight solo se abrirá cuando lo inicies tú.
Could not save your choice:|No se pudo guardar tu elección:
Focus reminders could not be enabled:|No se pudieron activar los recordatorios:
FlowSight will open at your next computer sign-in|FlowSight se abrirá en tu próximo inicio de sesión
Could not enable launch at sign-in:|No se pudo activar el inicio automático:
Could not change launch at sign-in:|No se pudo cambiar el inicio automático:
Could not change automatic tracking:|No se pudo cambiar el seguimiento automático:
Could not change focus reminders:|No se pudieron cambiar los recordatorios:
Could not change reminder details:|No se pudieron cambiar los detalles del recordatorio:
Automatic tracking could not start:|No se pudo iniciar el seguimiento automático:
On — aggregate summaries are sent to your FlowSight Cloud account.|Activado: se envían resúmenes agregados a tu cuenta de FlowSight Cloud.
On — Coach can process messages and selected work context in the cloud. The proactive agent stays local.|Activado: Coach puede procesar mensajes y contexto seleccionado en la nube. El agente proactivo sigue siendo local.
Off — Coach cannot receive messages or activity.|Desactivado: Coach no puede recibir mensajes ni actividad.
I understand — enable at sign-in|Lo entiendo: activar al iniciar sesión
Could not save the privacy acknowledgement:|No se pudo guardar la aceptación de privacidad:
Sign in with a plan that includes cloud sync first.|Primero inicia sesión con un plan que incluya sincronización en la nube.
Cloud activity sync enabled|Sincronización en la nube activada
Cloud activity sync disabled|Sincronización en la nube desactivada
Sign in with a plan that includes cloud AI first.|Primero inicia sesión con un plan que incluya IA en la nube.
Cloud AI sharing enabled|Envío a IA en la nube activado
Cloud AI sharing disabled|Envío a IA en la nube desactivado
Window titles will be stored locally|Los títulos de ventanas se guardarán localmente
Stored window titles removed|Títulos de ventanas guardados eliminados
Excluded applications updated|Aplicaciones excluidas actualizadas
Local retention updated and enforced|Plazo de conservación local actualizado y aplicado
Data export saved to Downloads|Exportación guardada en Descargas
Data export failed:|Error al exportar los datos:
This permanently erases local activity, preferences, chat history, credentials, and logs. Type DELETE to continue.|Esto borra permanentemente la actividad local, las preferencias, el historial de chat, las credenciales y los registros. Escribe DELETE para continuar.
Local erasure failed:|Error al borrar datos locales:
This permanently deletes your FlowSight cloud account and its personal data, then erases this device. Type DELETE to continue.|Esto elimina permanentemente tu cuenta de FlowSight y sus datos personales, y después borra los datos de este dispositivo. Escribe DELETE para continuar.
Account deletion failed:|Error al eliminar la cuenta:
You have not chosen yet. The prompt appears on first launch.|Todavía no has elegido. La pregunta aparece en el primer inicio.
Pseudonymous usage data is shared to improve FlowSight. You can withdraw at any time.|Se comparten datos de uso seudónimos para mejorar FlowSight. Puedes retirar el consentimiento cuando quieras.
Product analytics is not shared.|No se comparten analíticas de producto.
Thanks for helping improve FlowSight|Gracias por ayudar a mejorar FlowSight
Pseudonymous analytics turned off|Analíticas seudónimas desactivadas
Could not save analytics preference:|No se pudo guardar la preferencia de analíticas:
Please enter at least a few words of feedback.|Escribe al menos unas palabras de comentario.
Thanks — your feedback was saved.|Gracias: tus comentarios se han guardado.
Could not send feedback:|No se pudieron enviar los comentarios:
Unknown|Desconocido
Other (Type Manual)|Otra (escribir manualmente)
Manual Task|Tarea manual
Choose a folder before turning on automatic reports.|Elige una carpeta antes de activar los informes automáticos.
Weekly report schedule saved|Programación del informe semanal guardada
Could not save schedule:|No se pudo guardar la programación:
Could not open the last report folder:|No se pudo abrir la carpeta del último informe:
Generating and saving this week's work report…|Generando y guardando el informe de esta semana…
Weekly work report saved|Informe semanal guardado
Automatic report could not be saved:|No se pudo guardar el informe automático:
Automatic work report failed:|Error del informe automático:
Starting local AI engine…|Iniciando el motor local de IA…
Section — project summary|Sección: resumen del proyecto
Section — overall workflow health|Sección: revisión general del trabajo
Section — health breakdown table|Sección: detalle de áreas de trabajo
Section — timeline review|Sección: revisión temporal
Section — known issues|Sección: problemas conocidos
Section — potential risks|Sección: riesgos posibles
Section — progress & observed work|Sección: progreso y trabajo observado
Section — lessons & recommendations|Sección: aprendizajes y recomendaciones
Unavailable|No disponible
Connected|Conectado
The local browser bridge is unavailable. Restart FlowSight and try again.|La conexión local con el navegador no está disponible. Reinicia FlowSight e inténtalo de nuevo.
The store controls installation. If its page is not available yet, Google or Microsoft may still be reviewing the extension.|La tienda controla la instalación. Si la página no está disponible, Google o Microsoft pueden estar revisando la extensión.
The official store listing is not available in this build yet. Other local tools still work.|La página oficial de la tienda aún no está disponible en esta versión. Las demás herramientas locales siguen funcionando.
Ready. You can use browser actions from local automations.|Listo. Puedes usar acciones del navegador en las automatizaciones locales.
Ready.|Listo.
The connection was lost. Reopen the extension and choose Save and connect.|Se perdió la conexión. Abre de nuevo la extensión y elige Guardar y conectar.
Pair the browser extension first.|Vincula primero la extensión del navegador.
Forget|Olvidar
Saved preferences could not be loaded.|No se pudieron cargar las preferencias guardadas.
The pairing key is unavailable. Restart FlowSight and try again.|La clave de vinculación no está disponible. Reinicia FlowSight e inténtalo de nuevo.
Key copied. Paste it into the extension’s pairing page and choose Save and connect.|Clave copiada. Pégala en la página de vinculación de la extensión y elige Guardar y conectar.
Could not copy automatically. Select the hidden key field above and copy it manually.|No se pudo copiar automáticamente. Selecciona el campo de clave oculto y cópialo manualmente.
Checking the local connection…|Comprobando la conexión local…
Not connected yet. Choose Save and connect in the extension, then check again.|Todavía no está conectado. Elige Guardar y conectar en la extensión y vuelve a comprobarlo.
Action cancelled.|Acción cancelada.
No task time recorded yet|Todavía no hay tiempo de tareas registrado
Deep Focus is unavailable until local activity has been analyzed|La concentración sostenida no está disponible hasta analizar la actividad local
No hourly deep-focus data available yet|Todavía no hay datos horarios de concentración sostenida
under 1 minute|menos de 1 minuto
Hourly focus|Concentración por hora
60m max|máximo 60 min
Timeline|Cronología
General|General
Work report|Informe de trabajo
Generate AI work report|Generar informe de trabajo con IA
This week|Esta semana
Activity this week|Actividad de esta semana
No activity recorded today. Start a tracking block to see your work patterns here.|Hoy no hay actividad registrada. Inicia un bloque de seguimiento para ver aquí tus hábitos de trabajo.
Canonical sustained non-work browsing episodes|Episodios sostenidos de navegación ajena al trabajo
Start tracking to see your focus insights.|Inicia el seguimiento para ver tus datos de concentración.
Sustained focus share|Proporción de concentración sostenida
Highlights|Resumen
No daily goal set|Sin objetivo diario
Daily goal progress|Progreso del objetivo diario
Building your work report|Preparando tu informe de trabajo
Knowledge worker|Profesional
PDF export failed:|Error al exportar PDF:
Generating…|Generando…
Connecting…|Conectando…
Complete|Completado
Basic report ready (AI unavailable)|Informe básico listo (IA no disponible)
Report downloaded|Informe descargado
Your PDF is ready in your Downloads folder.|Tu PDF está listo en la carpeta Descargas.
Open folder|Abrir carpeta
Could not open folder:|No se pudo abrir la carpeta:
Generate a report first|Genera primero un informe
Manual code login|Inicio de sesión con código manual
Enter your access token directly|Introduce directamente tu token de acceso
Please enter a valid code|Introduce un código válido
Verifying...|Verificando…
Login Successful|Sesión iniciada
Login Failed:|Error al iniciar sesión:
Login with Code|Iniciar sesión con código
Choose valid start and end times.|Elige horas de inicio y fin válidas.
Choose valid times.|Elige horas válidas.
That time is unavailable because the clocks change.|Esa hora no está disponible debido al cambio de horario.
Choose a start later today.|Elige un inicio posterior a la hora actual de hoy.
Choose 15 minutes to 16 hours, ending today.|Elige entre 15 minutos y 16 horas, con final hoy.
Planning on this device…|Planificando en este dispositivo…
Describe what you want to change.|Describe qué quieres cambiar.
Describe what you want to work on today.|Describe en qué quieres trabajar hoy.
The local agent is considering your available hours, tasks and saved preferences…|El agente local está considerando tus horas disponibles, tareas y preferencias guardadas…
Work that needs more time|Trabajo que necesita más tiempo
Draft ready. Review the times and estimates before adding it to your local calendar.|Borrador listo. Revisa las horas y estimaciones antes de añadirlo al calendario local.
This draft expired. Suggest a fresh session.|Este borrador ha caducado. Solicita una nueva sesión.
Session details changed. Suggest a fresh plan before confirming.|Los detalles de la sesión han cambiado. Solicita un nuevo plan antes de confirmar.
Draft discarded. Edit your request to make another plan.|Borrador descartado. Edita tu solicitud para crear otro plan.
FlowSight needs a repair|FlowSight necesita una reparación
Your personal data stays in place|Tus datos personales se conservan
Repair details|Detalles de la reparación
Your activity history, settings and license are kept outside the app installation.|Tu historial, ajustes y licencia se guardan fuera de la instalación de la aplicación.
Preparing repair…|Preparando reparación…
Use app for now|Usar la aplicación por ahora
Repair download|Descarga de reparación
Some app files need replacing. Reinstall FlowSight from your Microsoft Store Library to restore them.|Hay archivos de la aplicación que deben sustituirse. Reinstala FlowSight desde tu biblioteca de Microsoft Store para restaurarlos.
Some app files need replacing. FlowSight will download a verified copy and reinstall the app automatically.|Hay archivos que deben sustituirse. FlowSight descargará una copia verificada y reinstalará la aplicación automáticamente.
Retry repair|Reintentar reparación
Downloading a verified copy…|Descargando una copia verificada…
Reinstalling FlowSight… it will restart shortly.|Reinstalando FlowSight… se reiniciará en breve.
The repair could not finish. Check your connection and try again.|No se pudo terminar la reparación. Comprueba la conexión e inténtalo de nuevo.
The work mix had a clear centre|La distribución del trabajo tenía un centro claro
Sustained work was visible|Se observó trabajo sostenido
Coverage limits the conclusion|La cobertura limita las conclusiones
Recorded time is a starting point|El tiempo registrado es un punto de partida
Foreground destination analysis could not be completed. Try generating the report again.|No se pudo completar el análisis de aplicaciones activas. Genera el informe de nuevo.
This report predates app and site context analysis. Generate a new report to see it.|Este informe es anterior al análisis de aplicaciones y sitios. Genera uno nuevo para verlo.
No recurring work-to-app return or sustained casual-browsing destination was observed in this period.|No se observaron regresos recurrentes entre trabajo y aplicaciones ni navegación informal sostenida durante este periodo.
No assessment available|Sin evaluación disponible
Unlabelled|Sin etiqueta
Daily work review|Revisión diaria de trabajo
Weekly work review|Revisión semanal de trabajo
The day in view|El día de un vistazo
The week in view|La semana de un vistazo
Activity was recorded in this period.|Se registró actividad en este periodo.
No local activity was recorded in this period.|No se registró actividad local en este periodo.
Work area|Área de trabajo
Observed|Observado
No activity was recorded, so there is not enough evidence to draw a lesson for this period.|No se registró actividad, por lo que no hay evidencia suficiente para extraer un aprendizaje de este periodo.
0.0 hours|0,0 horas
No category time recorded.|Sin tiempo registrado por categoría.
No specific next move is supported by this period yet. Keep tracking to build a baseline.|Todavía no hay evidencia para proponer un siguiente paso concreto. Sigue registrando actividad para crear una referencia.
No work-area detail was generated.|No se generó detalle por área de trabajo.
Next session|Próxima sesión
Review summary|Resumen de la revisión
Tracked time|Tiempo registrado
Days with activity|Días con actividad
What to do next|Qué hacer ahora
Actions suggested by the recorded evidence|Acciones sugeridas por la actividad registrada
Attention detours|Desvíos de atención
Observed visits and returns between work screens|Visitas y regresos observados entre pantallas de trabajo
Recorded time, not a productivity score|Tiempo registrado, sin puntuación de productividad
Activity by day|Actividad por día
Time by category|Tiempo por categoría
How to read the signal|Cómo interpretar la señal
Work-area detail|Detalle por área de trabajo
Specific observations behind the review|Observaciones concretas de la revisión
Work observed|Trabajo observado
Observed work and watchpoints|Trabajo observado y aspectos a revisar
Watchpoints|Aspectos a revisar
Known issues|Problemas conocidos
Potential risks|Riesgos posibles
What this period taught us|Aprendizajes de este periodo
Download PDF|Descargar PDF
Download weekly work review as PDF|Descargar la revisión semanal en PDF
No dated activity available.|Sin actividad con fecha disponible.
No additional interpretation was generated.|No se generó interpretación adicional.
Focus target|Objetivo de concentración
No labelled work was observed.|No se observó trabajo etiquetado.
None flagged.|Ninguno señalado.
Narrative assisted by local AI.|Texto asistido por IA local.
Structured, rule-based narrative.|Texto estructurado basado en reglas.
App name unavailable in this PDF font - see on-screen report|Nombre de aplicación no disponible en esta fuente: consulta el informe en pantalla
this app|esta aplicación
Current period|Periodo actual
TRACKED TIME|TIEMPO REGISTRADO
DAYS WITH ACTIVITY|DÍAS CON ACTIVIDAD
No labelled work observed.|Sin trabajo etiquetado observado.
Local data · Local AI narrative · Interpret with context|Datos locales · Texto con IA local · Interpretar con contexto
Local data · Rule-based narrative · Interpret with context|Datos locales · Texto basado en reglas · Interpretar con contexto
Supabase public configuration is missing.|Falta la configuración pública de Supabase.
Invalid email or password. Check your license credentials.|Correo o contraseña incorrectos. Comprueba las credenciales de tu licencia.
Please confirm your email before signing in.|Confirma tu correo antes de iniciar sesión.
This account does not have an active Individual license.|Esta cuenta no tiene una licencia Individual activa.
No active Individual or Team license found for this account.|No se encontró una licencia Individual o Team activa para esta cuenta.
Cloud login is not configured yet. Contact support.|El inicio de sesión en la nube no está configurado. Contacta con soporte.
Login failed. Please try again or contact support.|No se pudo iniciar sesión. Inténtalo de nuevo o contacta con soporte.
Could not load license entitlements.|No se pudieron cargar los permisos de la licencia.
Could not create your personal team.|No se pudo crear tu equipo personal.
Could not claim license code.|No se pudo activar el código de licencia.
Login failed. Please try again.|No se pudo iniciar sesión. Inténtalo de nuevo.
We could not identify this account. Please try again.|No se pudo identificar esta cuenta. Inténtalo de nuevo.
We could not load your profile.|No se pudo cargar tu perfil.
This Team license account is not assigned to a team yet. Ask your PM for an invitation code.|Esta cuenta Team aún no está asignada a un equipo. Pide un código de invitación al responsable.
We could not load your team membership.|No se pudo cargar tu pertenencia al equipo.
Language|Idioma
Today, {p0}|Hoy, {p0}
{p0} elapsed of {p1} scheduled|{p0} transcurridos de {p1} previstos
Startup & focus reminders|Inicio y recordatorios de concentración
App language|Idioma de la aplicación
Use system language|Usar idioma del sistema
Language changes immediately. Your current tracking session and drafts stay open.|El idioma cambia al momento. La sesión de seguimiento y los borradores siguen abiertos.
Language saved|Idioma guardado
This language applies for this session. Could not save it on this device.|Este idioma se aplica a esta sesión. No se pudo guardarlo en el dispositivo.
Preparing local AI…|Preparando la IA local…
This may take a few minutes|Puede tardar unos minutos
Privacy & data|Privacidad y datos
Write proposal|Escribir propuesta
 Check your choices and try again.| Comprueba tus opciones e inténtalo de nuevo.
 Edit the request and try again.| Edita la solicitud e inténtalo de nuevo.
{p0} Calendar OAuth is not configured in this build.|OAuth del calendario de {p0} no está configurado en esta versión.
Now · {p0} Calendar|Ahora · Calendario de {p0}
{p0}{p1}{p2} connected · {p3}|{p0}{p1}{p2} conectado · {p3}
Finish {p0} authorization in your browser.|Completa la autorización de {p0} en tu navegador.
Connected to {p0}|Conectado a {p0}
Last published {p0} to {p1}{p2}|Última publicación del {p0} al {p1}{p2}
{p0} hours|{p0} horas
{p0} day{p1}|{p0} día{p1}
Linking... ({p0}/30)|Vinculando… ({p0}/30)
Coach: {p0}/{p1} prompts this month · {p2}|Coach: {p0}/{p1} consultas este mes · {p2}
{p0} left|{p0} restantes
Version {p0}|Versión {p0}
Downloading… {p0}%|Descargando… {p0}%
Local AI Server Ready{p0}|Servidor local de IA listo{p0}
Name: {p0}|Nombre: {p0}
Focus: {p0}|Trabajo: {p0}
Goals: {p0}|Objetivos: {p0}
Daily goal: {p0}h|Objetivo diario: {p0} h
{p0} of 5|{p0} de 5
Forget {p0}|Olvidar {p0}
Tools unavailable: {p0}|Herramientas no disponibles: {p0}
{p0} focus reminder(s) held during quiet mode|{p0} recordatorios de concentración retenidos durante el modo silencio
Failed to load summary: {p0}|No se pudo cargar el resumen: {p0}
Downloading a verified copy… {p0}%|Descargando una copia verificada… {p0}%
{p0} blocks added to your FlowSight calendar.|{p0} bloques añadidos al calendario de FlowSight.
{p0} Edit the request and try again.|{p0} Edita la solicitud e inténtalo de nuevo.
{p0} ({p1}% of tracked time) was categorised as {p2}. Compare that mix with your intended priorities; time distribution alone is not an outcome measure.|{p0} ({p1}% del tiempo registrado) se clasificó como {p2}. Compara esa distribución con tus prioridades; el reparto del tiempo por sí solo no mide los resultados.
 across {p0} sustained blocks| en {p0} bloques de concentración sostenida
{p0}h of sustained focus was recorded{p1}. Use the recorded block boundaries to identify conditions worth repeating, without treating duration as a productivity score.|Se registraron {p0} h de concentración sostenida{p1}. Usa los límites de los bloques para identificar condiciones que merezca la pena repetir, sin convertir la duración en una puntuación de productividad.
Activity was recorded on {p0} of {p1} days. Days without recorded activity do not prove that no work happened.|Se registró actividad en {p0} de {p1} días. Los días sin actividad registrada no demuestran que no se trabajara.
{p0}h was recorded, but category and focus signals are too limited for a specific workflow conclusion. Add task context or compare another period before changing plans.|Se registraron {p0} h, pero las categorías y señales de concentración son insuficientes para sacar una conclusión concreta. Añade contexto de las tareas o compara otro período antes de cambiar tus planes.
Choose a playlist in {p0} before the next work block, leave playback running, and batch track changes into one break. Check whether you return less often next session.|Elige una lista en {p0} antes del siguiente bloque, deja la reproducción activa y reserva los cambios de canción para un descanso. Comprueba si vuelves menos veces en la próxima sesión.
Choose a playlist in {p0} before the work block and leave playback in the background; save track changes for a break.|Elige una lista en {p0} antes del bloque de trabajo y deja la reproducción en segundo plano; reserva los cambios de canción para un descanso.
Queue or save what you want to watch in {p0} for a planned break, then close it during the work block.|Guarda lo que quieras ver en {p0} para un descanso previsto y cierra la aplicación durante el bloque de trabajo.
If {p0} is not needed for live collaboration, mute it for the next focus block and check it at a chosen interval; keep urgent contacts available.|Si no necesitas {p0} para colaborar en directo, siléncialo durante el próximo bloque y revísalo en un momento elegido; mantén disponibles los contactos urgentes.
Decide whether {p0} belongs to the current task. If not, close it for one focus block and move optional checks to a planned break.|Decide si {p0} forma parte de la tarea actual. Si no, ciérralo durante un bloque y reserva las consultas opcionales para un descanso previsto.
 and | y 
{p0} and {p1}|{p0} y {p1}
 around {p0}| hacia las {p0}
{p0}: {p1} foreground sightings{p2} ({p3} sampled).|{p0}: {p1} observaciones en primer plano{p2} ({p3} muestreados).
 Work screens appeared between sightings; {p0} reappeared {p1}{p2}.| Aparecieron pantallas de trabajo entre las observaciones; {p0} reapareció {p1}{p2}.
once|una vez
{p0} times|{p0} veces
, with the shortest interval {p0} min|, con un intervalo mínimo de {p0} min
sighting|observación
sightings|observaciones
day|día
days|días
{p0} foreground {p1} across {p2} {p3}; {p4} on screen. |{p0} {p1} en primer plano en {p2} {p3}; {p4} en pantalla. 
{p0} revisits had work screens in between.|En {p0} visitas hubo pantallas de trabajo entre medias.
Based on sampled foreground screens; background playback is not included.|Basado en muestras de pantallas en primer plano; no incluye reproducción en segundo plano.
{p0}% of tracked time|{p0}% del tiempo registrado
Owner: {p0}|Responsable: {p0}
{p0}: {p1} in sampled foreground visits|{p0}: {p1} en visitas muestreadas en primer plano
Generated {p0}|Generado el {p0}
Sustained focus · {p0} blocks|Concentración sostenida · {p0} bloques
Based on activity stored on this device. {p0} Interpret alongside your own context.|Basado en la actividad guardada en este dispositivo. {p0} Interprétalo junto con tu propio contexto.
Next session: {p0}|Próxima sesión: {p0}
SUSTAINED FOCUS · {p0} BLOCKS|CONCENTRACIÓN SOSTENIDA · {p0} BLOQUES
Focus target: {p0}|Objetivo de concentración: {p0}
{p0} active day{p1}|{p0} día{p1} con actividad
Daily goal {p0} hours|Objetivo diario: {p0} horas
At least {p0} observed minutes of non-work Browsing in one episode; capture gaps up to {p1} seconds may be joined|Al menos {p0} minutos observados de navegación ajena al trabajo en un episodio; se pueden unir intervalos de captura de hasta {p1} segundos
{p0} across {p1} sustained blocks today.|{p0} en {p1} bloques de concentración sostenida hoy.
Focused activity is present, but no block has reached the {p0}-minute reference yet.|Hay actividad de concentración, pero ningún bloque ha alcanzado aún la referencia de {p0} minutos.
{p0}% of tracked time in sustained focus|{p0}% del tiempo registrado en concentración sostenida
{p0} in sustained blocks · {p1} tracked|{p0} en bloques sostenidos · {p1} registrados
Step {p0} of {p1}|Paso {p0} de {p1}
FlowSight — Understand your own Workflows|FlowSight — Comprende tu forma de trabajar
1 of 5|1 de 5
With your permission, FlowSight can open when you sign in to this computer. Closing its window will keep it in the system tray; Quit in the tray menu exits fully.|Con tu permiso, FlowSight puede abrirse al iniciar sesión en este equipo. Al cerrar la ventana, seguirá en la bandeja del sistema; Salir en el menú de la bandeja cierra la aplicación por completo.
Common password managers are excluded by default; add other sensitive apps in Privacy & data.|Los gestores de contraseñas habituales están excluidos por defecto; añade otras aplicaciones sensibles en Privacidad y datos.
Effective 23 August 2026 · Controller legal identity and address: must be completed in the published notice before production · Privacy contact: manuel@flowsight.site.|Vigente desde el 23 de agosto de 2026 · Identidad legal y dirección del responsable: deben completarse en el aviso publicado antes de producción · Contacto de privacidad: manuel@flowsight.site.
When you start tracking, the app processes images of the active window, active app and interface context on your device to create activity summaries. Images and raw interaction events stay in volatile memory and are discarded after local analysis. Excluded applications are skipped. This processing provides the feature you request. Summaries remain on your device for your selected retention period. If you enable focus reminders, local Qwen receives only verified aggregate counts of app switches or non-work browsing episodes. It can propose a reminder through an internal tool; FlowSight checks permission and cooldown before showing it. Reminders use generic wording by default. If you separately enable contextual reminders in Profile, the system notification may show a public app or site name and your selected task, but never a window title or URL. Those names stay on your device and are not sent to Qwen for reminder decisions.|Al iniciar el seguimiento, la aplicación procesa en tu dispositivo imágenes de la ventana activa, la aplicación activa y el contexto de la interfaz para crear resúmenes de actividad. Las imágenes y los eventos de interacción originales permanecen en memoria volátil y se descartan tras el análisis local. Las aplicaciones excluidas se omiten. Este procesamiento proporciona la función solicitada. Los resúmenes permanecen en tu dispositivo durante el plazo de conservación elegido. Si activas los recordatorios de concentración, Qwen local solo recibe recuentos agregados verificados de cambios de aplicación o episodios de navegación ajena al trabajo. Puede proponer un recordatorio mediante una herramienta interna; FlowSight comprueba el permiso y el intervalo de espera antes de mostrarlo. Por defecto, los recordatorios usan texto genérico. Si activas por separado los recordatorios con contexto en Perfil, la notificación del sistema puede mostrar un nombre público de aplicación o sitio y tu tarea seleccionada, pero nunca el título de una ventana ni una URL. Esos nombres permanecen en tu dispositivo y no se envían a Qwen para decidir los recordatorios.
Integrations:|Integraciones:
Several events overlap right now. Describe the task below; FlowSight will not guess which event is yours.|Hay varios eventos simultáneos. Describe la tarea a continuación; FlowSight no elegirá un evento por ti.
The local agent works in the background. It can suggest focus reminders while tracking; actions that change your work still need your confirmation.|El agente local trabaja en segundo plano. Puede sugerir recordatorios durante el seguimiento; las acciones que modifican tu trabajo siguen necesitando tu confirmación.
. If you closed that page, click the FlowSight extension icon to reopen it.|. Si cerraste esa página, pulsa el icono de la extensión FlowSight para abrirla de nuevo.
The key stays on this device. The extension communicates with FlowSight through 127.0.0.1; browser actions still follow your confirmation settings.|La clave permanece en este dispositivo. La extensión se comunica con FlowSight mediante 127.0.0.1; las acciones del navegador siguen tus ajustes de confirmación.
Choose what FlowSight does when you sign in. Closing the window keeps it in the system tray; use Quit in the tray menu to exit.|Elige qué hace FlowSight al iniciar sesión. Al cerrar la ventana, permanece en la bandeja del sistema; usa Salir en el menú de la bandeja para cerrarlo.
Optional. FlowSight uses a short public app or site label and your chosen task in system notifications. Window titles and URLs are never shown; Qwen still sees only aggregate counts. System notifications can appear on your lock screen, depending on your device settings.|Opcional. FlowSight usa una etiqueta pública breve de la aplicación o sitio y tu tarea elegida en las notificaciones del sistema. Nunca se muestran títulos de ventanas ni URL; Qwen sigue recibiendo solo recuentos agregados. Las notificaciones pueden aparecer en la pantalla de bloqueo, según los ajustes del dispositivo.
Transport: STDIO. Argument:|Transporte: STDIO. Argumento:
Retry repair|Reintentar reparación
Loading...|Cargando…
. Check your choices and try again.|. Comprueba tus opciones e inténtalo de nuevo.
. FlowSight will retry.|. FlowSight lo intentará de nuevo.
. Check the schedule for the next run.|. Comprueba la programación de la próxima ejecución.
1 hour|1 hora
2 hours|2 horas
3 hours|3 horas
4 hours|4 horas
5 hours|5 horas
6 hours|6 horas
7 hours|7 horas
8 hours|8 horas
9 hours|9 horas
10 hours|10 horas
11 hours|11 horas
12 hours|12 horas
One-time {p0} download. Your screen stays on this device.|Descarga única de {p0}. Las imágenes de tu pantalla permanecen en este dispositivo.
Downloading local AI model… {p0}%|Descargando el modelo local de IA… {p0}%
File {p0} of {p1} · {p2} / {p3}|Archivo {p0} de {p1} · {p2} / {p3}
Checking file {p0} of {p1} against SHA-256.|Verificando el archivo {p0} de {p1} con SHA-256.
Trying {p0} ({p1}/{p2})|Probando {p0} ({p1}/{p2})
 Last saved {p0}.| último guardado: {p0}.
Scheduled for {p0} at {p1}.{p2}|Programado para el {p0} a las {p1}.{p2}
Automatic reports are off.{p0}|Los informes automáticos están desactivados.{p0}
Focus block {p0}: {p1}{p2}|Bloque de concentración {p0}: {p1}{p2}
Held reminders: {p0}|Recordatorios retenidos: {p0}
Focus block ended{p0}|Bloque de concentración finalizado{p0}
; {p0} held reminder(s) delivered|; {p0} recordatorios retenidos enviados
{p0}: restore ID {p1}|{p0}: ID de restauración {p1}
{p0}: opened {p1}|{p0}: abierto {p1}
{p0}: done|{p0}: completado
Open tasks for tomorrow: {p0}|Tareas abiertas para mañana: {p0}
Task {p0}: {p1}|Tarea {p0}: {p1}
Calendar event: {p0} ({p1} to {p2})|Evento del calendario: {p0} ({p1} a {p2})
Draft saved for {p0}. It has not been sent.|Borrador guardado para {p0}. No se ha enviado.
Created {p0} child {p1} under {p2}{p3}{p4}|Elemento de {p0} {p1} creado dentro de {p2}{p3}{p4}
Opened {p0}|Abierto {p0}
 Pair the browser extension first.| Vincula primero la extensión del navegador.
Deep Focus is unavailable until local activity has been analyzed|La concentración profunda no está disponible hasta que se analice la actividad local
No sustained {p0}-minute block yet today|Hoy aún no hay un bloque sostenido de {p0} minutos
No hourly deep-focus data available yet|Todavía no hay datos por hora de concentración profunda
{p0} minutes|{p0} minutos
{p0} — {p1} of deep focus|{p0} — {p1} de concentración profunda
{p0} — no deep focus|{p0} — sin concentración profunda
Analysis|Análisis
Coding|Programación
Debugging|Depuración
Code Review|Revisión de código
Testing|Pruebas
DevOps|DevOps
Database|Base de datos
Research|Investigación
Documentation|Documentación
Planning|Planificación
Communication|Comunicación
Meeting|Reunión
Admin|Administración
Browsing|Navegación
Idle|Inactividad
General|General
none|ninguna
running|activo
paused|en pausa
ended|finalizado
open|abierta
in_progress|en curso
completed|completada
Start a timed focus block with a specific intention. Ask the user to confirm first.|Inicia un bloque de concentración con una intención concreta. Pide confirmación primero.
Resume the paused focus block with its remaining time and protections.|Reanuda el bloque en pausa con su tiempo restante y sus protecciones.
Pause the current focus block and tracking.|Pausa el bloque actual y el seguimiento.
End the current focus block and tracking.|Finaliza el bloque actual y el seguimiento.
Silence Windows app notification banners or restore their previous setting. This does not configure Focus Assist allowlists.|Silencia las notificaciones emergentes de Windows o restaura el ajuste anterior. No configura las listas permitidas del Asistente de concentración.
Temporarily block specified browser domains or URL paths in the paired extension.|Bloquea temporalmente los dominios o rutas indicados en la extensión vinculada.
Remove temporary blocks for specified domains or URL paths.|Elimina los bloqueos temporales de los dominios o rutas indicados.
Close a specific browser tab after confirmation and keep a restore record.|Cierra una pestaña concreta tras confirmar y conserva un registro para restaurarla.
List open browser tabs and FlowSight restore records so the user can identify a tab before closing it.|Muestra las pestañas abiertas y los registros de restauración de FlowSight para identificar una pestaña antes de cerrarla.
Restore a tab previously closed by FlowSight.|Restaura una pestaña que FlowSight cerró previamente.
Create a local task from a concrete intention.|Crea una tarea local a partir de una intención concreta.
List recent local tasks and their IDs so one can be updated or completed.|Muestra las tareas locales recientes y sus ID para actualizarlas o completarlas.
Update a local task title, due time, or status.|Actualiza el título, fecha límite o estado de una tarea local.
Mark a local task completed.|Marca una tarea local como completada.
Set a local task priority from 1 (highest) to 5.|Asigna una prioridad a una tarea local: de 1 (máxima) a 5.
Find free time in the connected calendar for a given interval.|Busca tiempo libre en el calendario conectado para un intervalo.
List FlowSight-owned local and connected calendar events with their IDs.|Muestra los eventos locales y conectados de FlowSight con sus ID.
Read the current connected calendar event, its task description, time, and organizer status. No mutation.|Lee el evento actual del calendario conectado, su descripción, hora y organizador. No modifica datos.
Create a focus event in FlowSight's local calendar. Connected calendar writes are unavailable in this release.|Crea un evento de concentración en el calendario local de FlowSight. Esta versión no permite escribir en calendarios conectados.
Move a FlowSight-owned local event. Connected calendar writes are unavailable in this release.|Mueve un evento local de FlowSight. Esta versión no permite escribir en calendarios conectados.
Show the notifications FlowSight held during a focus block.|Muestra las notificaciones retenidas por FlowSight durante un bloque.
Prepare a response without sending it. Email recipients are addresses; Slack recipients are channel or user IDs; Teams recipients are chat IDs.|Prepara una respuesta sin enviarla. El destinatario de correo es una dirección; el de Slack es un ID de canal o usuario; el de Teams es un ID de chat.
List recent saved message drafts and their delivery state.|Muestra los borradores recientes y su estado de envío.
Send one saved draft through its connected provider. The full recipient and body are shown before confirmation.|Envía un borrador con el proveedor conectado. Muestra el destinatario y contenido completos antes de confirmar.
Read the current local project and linked work item context.|Lee el proyecto local actual y el contexto de su elemento de trabajo vinculado.
Format a PR description from supplied, verified changes and test results. This only returns a draft for review; it does not publish a PR.|Da formato a una descripción de PR usando los cambios y pruebas verificados. Devuelve un borrador para revisar; no publica una PR.
Open a user-specified local file, project, or HTTPS resource.|Abre un archivo local, proyecto o recurso HTTPS indicado por el usuario.
Run an auditable deep work, end-of-day, or recover-focus routine.|Ejecuta una rutina registrada de trabajo profundo, fin del día o recuperación de la concentración.
Remember a user-approved behavior rule on this device.|Guarda una regla de comportamiento aprobada por el usuario en este dispositivo.
Forget a previously saved behavior rule.|Olvida una regla de comportamiento guardada.
Show all behavior rules FlowSight currently remembers.|Muestra todas las reglas de comportamiento guardadas por FlowSight.
Choose breaks between 5 and 30 minutes.|Elige descansos de entre 5 y 30 minutos.
The local AI must identify your work and allow 5–30 minute breaks. Try adding estimates.|La IA local debe identificar tus tareas y reservar descansos de 5 a 30 minutos. Prueba a añadir estimaciones.
The local AI could not apply that revision. Specify a topic first, task minutes, or break minutes and try again.|La IA local no pudo aplicar ese cambio. Indica qué tema va primero, los minutos de las tareas o los minutos de descanso e inténtalo de nuevo.
The local AI could not interpret a fixed time safely. Add the commitment to your local calendar and regenerate, or restate its HH:MM range.|La IA local no pudo interpretar una hora fija con seguridad. Añade el compromiso al calendario local y genera otro plan, o indica su intervalo HH:MM.
This plan is no longer pending.|Este plan ya no está pendiente.
This draft expired. Generate a fresh plan.|Este borrador ha caducado. Genera un plan nuevo.
The first block has already started. Adjust the session start and regenerate your plan.|El primer bloque ya ha empezado. Ajusta la hora de inicio y genera otro plan.
Install the release build before enabling launch at login.|Instala la versión publicada antes de activar el inicio automático.
Connected-service writes are unavailable in this release.|Esta versión no permite escribir en servicios conectados.
Connected-calendar writes are unavailable in this release. Select the FlowSight local calendar first.|Esta versión no permite escribir en calendarios conectados. Selecciona primero el calendario local de FlowSight.
A focus block is already active. End it before starting another.|Ya hay un bloque de concentración activo. Finalízalo antes de iniciar otro.
Strict protection needs one or more browser patterns to block.|La protección estricta necesita uno o más patrones del navegador que bloquear.
Deep work needs an intention and duration.|El trabajo profundo necesita una intención y una duración.
The local AI could not produce a complete plan. List each task separately and add estimates, then try again.|La IA local no pudo generar un plan completo. Enumera cada tarea y añade estimaciones; después inténtalo de nuevo.
Describe today's work in up to 3,000 characters.|Describe el trabajo de hoy en un máximo de 3.000 caracteres.
Choose 15 minutes to 16 hours within one day.|Elige entre 15 minutos y 16 horas dentro del mismo día.
The local AI must propose between 1 and 16 blocks. Try fewer tasks.|La IA local debe proponer entre 1 y 16 bloques. Prueba con menos tareas.
The proposed start time is invalid.|La hora de inicio propuesta no es válida.
The proposed end time is invalid.|La hora de fin propuesta no es válida.
The proposal has overlapping, oversized, or out-of-hours blocks. Adjust your request and try again.|La propuesta tiene bloques que se solapan, duran demasiado o están fuera del horario. Ajusta la solicitud e inténtalo de nuevo.
A proposed block overlaps your local calendar. Ask for a different time.|Un bloque propuesto coincide con tu calendario local. Pide otra hora.
The local AI returned an incomplete plan. Add task durations and try again.|La IA local devolvió un plan incompleto. Añade la duración de las tareas e inténtalo de nuevo.
The local AI proposed work that does not match your request or has invalid estimates. Try again.|La IA local propuso tareas que no coinciden con tu solicitud o con estimaciones no válidas. Inténtalo de nuevo.
A fixed commitment time is invalid.|La hora de un compromiso fijo no es válida.
Fixed commitments must finish after they start.|Los compromisos fijos deben terminar después de empezar.
Choose a start later today so your plan can still be used.|Elige una hora de inicio posterior hoy para poder usar el plan.
Keep feedback under 2,000 characters.|Escribe los cambios en menos de 2.000 caracteres.
The local AI is unavailable.|La IA local no está disponible.
The local AI did not return a plan. Try adding task durations.|La IA local no devolvió un plan. Prueba a añadir la duración de las tareas.
The local AI returned an unsupported plan. Try again.|La IA local devolvió un plan no compatible. Inténtalo de nuevo.
The previous draft changed while planning. Generate a fresh plan.|El borrador anterior cambió durante la planificación. Genera otro plan.
Storage unavailable|Almacenamiento no disponible
`.trim().split('\n').map(line=>{const split=line.indexOf('|');return[line.slice(0,split),line.slice(split+1)];}));

spanish['{p0}\n\n{p1}\n\nDraft only; copy and review before publishing.']='{p0}\n\n{p1}\n\nSolo borrador; copia y revisa antes de publicar.';

Object.assign(spanish, {
  '1 of 6':'1 de 6','{p0} of 6':'{p0} de 6',
  'Total focus':'Concentración total','Total focus active':'Concentración total activa',
  'Use a domain or HTTP(S) path without spaces.':'Escribe un dominio o una ruta HTTP(S) sin espacios.',
  'Use a valid domain or HTTP(S) path.':'Escribe un dominio o una ruta HTTP(S) válidos.',
  'Use a public domain or HTTP(S) path, without a port, query, or fragment.':'Escribe un dominio público o una ruta HTTP(S) sin puerto, parámetros ni fragmento.',
  'Choose 5–180 minutes, 1–20 blocked sites, and up to 20 exceptions.':'Elige entre 5 y 180 minutos, de 1 a 20 páginas bloqueadas y hasta 20 excepciones.',
  'Describe your focus task in 1–160 characters.':'Describe tu tarea de concentración con entre 1 y 160 caracteres.',
  'Total focus is already active. End it before starting another session.':'La concentración total ya está activa. Finalízala antes de iniciar otra sesión.',
  'The extension did not apply total focus. Check its connection and try again.':'La extensión no aplicó la concentración total. Comprueba su conexión e inténtalo otra vez.',
  'Pair the FlowSight browser extension and leave the browser running first.':'Vincula la extensión de FlowSight y deja el navegador abierto primero.',
  'The browser extension did not answer within 90 seconds.':'La extensión del navegador no respondió en 90 segundos.',
  'Browser bridge unavailable.':'La conexión local con el navegador no está disponible.',
  'FlowSight focus reminders are held in your local digest during this session.':'Los recordatorios de FlowSight se guardan en tu resumen local durante esta sesión.',
  'Pages to block':'Páginas a bloquear','Allowed exceptions':'Excepciones permitidas',
  'One domain or path per line. Subdomains are included.':'Un dominio o ruta por línea. Se incluyen los subdominios.',
  'Exceptions take priority. Keep the pages you need for your work.':'Las excepciones tienen prioridad. Conserva las páginas que necesitas para trabajar.',
  'Session duration (minutes)':'Duración de la sesión (minutos)','Your focus task':'Tu tarea de concentración',
  'Start total focus':'Iniciar concentración total','End total focus':'Finalizar concentración total',
  'Save settings':'Guardar ajustes','Focus settings saved':'Ajustes de concentración guardados',
  'Checking browser protection…':'Comprobando la protección del navegador…',
  'Block distracting websites in your paired Chrome extension on Windows, macOS, or Linux.':'Bloquea páginas que te distraen con tu extensión de Chrome vinculada en Windows, macOS o Linux.',
  'Total focus active · browser block confirmed':'Concentración total activa · bloqueo confirmado',
  'Session active · browser protection not confirmed. Check the extension.':'Sesión activa · protección sin confirmar. Comprueba la extensión.',
  'Session ended · waiting for the extension to release protection.':'Sesión finalizada · esperando a que la extensión libere la protección.',
  'Browser connected · ready to start':'Navegador conectado · listo para empezar',
  'Connect Browser Controls to activate total focus.':'Conecta Browser Controls para activar la concentración total.',
  'Applying browser protection…':'Aplicando la protección del navegador…',
  'Browser protection confirmed. Your session is ready.':'Protección del navegador confirmada. Tu sesión está lista.',
  'Could not load focus settings. Try again.':'No se pudieron cargar los ajustes de concentración. Inténtalo otra vez.',
  'Could not update total focus:':'No se pudo actualizar la concentración total:',
  'Total focus ended':'Concentración total finalizada','Until':'Hasta',
  'Session ended. Reconnect the extension or use End total focus on a blocked page to release it now.':'Sesión finalizada. Reconecta la extensión o pulsa Finalizar concentración total en una página bloqueada para liberarla ahora.',
  'Protection lasts until the chosen end time, even if FlowSight closes. You can end it from a blocked page. Tracking remains a separate choice.':'La protección dura hasta la hora elegida, incluso si cierras FlowSight. Puedes finalizarla desde una página bloqueada. El seguimiento se activa por separado.',
  'Messaging replies':'Respuestas en mensajería','Coming later':'Próximamente',
  'A future integration could reply that you are in deep focus and optionally share your task. No automatic replies are sent.':'Una futura integración podrá responder que estás en concentración profunda y, si lo eliges, compartir tu tarea. No se envían respuestas automáticas.',
  'Page blocked':'Página bloqueada','Available for work':'Disponible para trabajar',
  'Example of total focus':'Ejemplo de concentración total',
  'Example · protect the pages you choose, keep your work accessible.':'Ejemplo · protege las páginas que elijas y mantén accesible tu trabajo.',
  'Make space for total focus':'Haz espacio para la concentración total',
  'Choose the websites to block during a focus session. Browser Controls works with Chrome on Windows, macOS, and Linux.':'Elige las páginas que quieres bloquear durante una sesión. Browser Controls funciona con Chrome en Windows, macOS y Linux.',
  'Connect Browser Controls':'Conectar Browser Controls',
  'Install the extension, copy the pairing key, and choose Save and connect in its options.':'Instala la extensión, copia la clave de vinculación y pulsa Guardar y conectar en sus opciones.',
  'Pairing key copied. Local port: 38547. Paste the key in the extension options.':'Clave copiada. Puerto local: 38547. Pega la clave en las opciones de la extensión.',
  'Could not copy the pairing key. Try again in Settings.':'No se pudo copiar la clave. Inténtalo otra vez en Ajustes.',
  'This step saves your settings. Start total focus from Today when you are ready. Tracking remains a separate choice.':'Este paso guarda tus ajustes. Inicia la concentración total desde Hoy cuando estés listo. El seguimiento se activa por separado.',
});

Object.assign(spanish, {
  'Ready. Browser actions and total focus are available.': 'Listo. Las acciones del navegador y la concentración total están disponibles.',
  'Browser connected. Update Browser Controls to enable total focus. In Arc, open arc://extensions and update the extension; approve any requested site access.': 'Navegador conectado. Actualiza Browser Controls para activar la concentración total. En Arc, abre arc://extensions y actualiza la extensión; acepta el acceso a los sitios si te lo solicita.',
  'Block distracting websites with Browser Controls in Arc on Windows or macOS, and Chrome on Windows, macOS, or Linux.': 'Bloquea páginas que te distraen con Browser Controls en Arc para Windows o macOS, y en Chrome para Windows, macOS o Linux.',
  'Choose the websites to block during a focus session. Browser Controls works with Arc on Windows and macOS, and Chrome on Windows, macOS, and Linux.': 'Elige las páginas que quieres bloquear durante una sesión. Browser Controls funciona con Arc en Windows y macOS, y con Chrome en Windows, macOS y Linux.',
  'Update Browser Controls in your browser and reconnect it before starting total focus.': 'Actualiza Browser Controls en tu navegador y vuelve a conectarlo antes de iniciar la concentración total.',
});
