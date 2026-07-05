import { createRoot } from 'react-dom/client';

import './arco-compat';
import '@arco-design/web-react/dist/css/arco.css';
import { FloatPanel } from './app/FloatPanel';
import { syncArcoTheme } from './app/theme';
import './panel.css';

// Mirror the user's appearance choice (driven by the main process) into Arco.
syncArcoTheme();

const container = document.getElementById('root');
if (container) {
    createRoot(container).render(<FloatPanel />);
}
