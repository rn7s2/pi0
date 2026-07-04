import { createRoot } from 'react-dom/client';

import './arco-compat';
import '@arco-design/web-react/dist/css/arco.css';
import { FloatPanel } from './app/FloatPanel';
import './panel.css';

// Match the main window's dark palette in the tray popup.
document.body.setAttribute('arco-theme', 'dark');

const container = document.getElementById('root');
if (container) {
    createRoot(container).render(<FloatPanel />);
}
