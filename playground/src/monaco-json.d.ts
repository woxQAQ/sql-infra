interface Window {
  MonacoEnvironment: {
    getWorker(moduleId: string, label: string): Worker;
  };
}
