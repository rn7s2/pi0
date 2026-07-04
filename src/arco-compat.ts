// Arco Design's imperative popups (Message, Notification, Modal.method) still
// call the legacy ReactDOM.render, which React 19 removed — so they throw
// "ReactDOM.render is not a function". Arco can use React 18+'s createRoot
// instead when handed one, but react-dom v19 no longer re-exports createRoot
// (only react-dom/client does), so Arco's own lookup comes back undefined.
// Register the client entry's createRoot here. Import this module *first* in
// every renderer entry, before any Arco imperative API can run.
import { setCreateRoot } from '@arco-design/web-react/es/_util/react-dom';
import { createRoot } from 'react-dom/client';

setCreateRoot(createRoot);
