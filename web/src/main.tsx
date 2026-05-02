import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App.tsx'
import { installDevServerTitleTracker } from './devServerTitle.ts'
import './global.css'

document.body.style.margin = '0';
document.body.style.padding = '0';
document.body.style.overflow = 'hidden';

installDevServerTitleTracker();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
