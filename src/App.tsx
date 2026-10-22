import { getCurrentWindow } from '@tauri-apps/api/window'
import { I18nProvider } from './i18n'
import { useSettings } from './hooks'
import Main from './views/Main'
import Selector from './views/Selector'
import Popup from './views/Popup'
import { ToastProvider } from './views/Toast'

function label(): string {
  try {
    return getCurrentWindow().label
  } catch {
    return 'main'
  }
}

const view = label()

export default function App() {
  const settings = useSettings(view)
  return (
    <I18nProvider lang={settings.uiLanguage}>
      {view === 'selector' ? (
        <Selector />
      ) : (
        <ToastProvider>
          {view === 'popup' ? <Popup settings={settings} /> : <Main settings={settings} />}
        </ToastProvider>
      )}
    </I18nProvider>
  )
}
