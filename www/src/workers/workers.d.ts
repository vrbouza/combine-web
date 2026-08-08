declare module "@/workers/Fitter.worker" {
  const WorkerFactory: new () => Worker;
  export default WorkerFactory;
}
