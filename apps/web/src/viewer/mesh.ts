import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

export interface MeshViewerHandle {
  reset(): void;
  setAutoRotate(enabled: boolean): void;
  dispose(): void;
}

/**
 * Plain three.js viewer for the optional GLB (the mesh extracted from the
 * splat by ComfyUI's SplatToMesh). Used only when the worker produced a GLB.
 */
export async function mountMeshViewer(container: HTMLElement, url: string): Promise<MeshViewerHandle> {
  container.replaceChildren();
  const width = Math.max(1, container.clientWidth);
  const height = Math.max(1, container.clientHeight);

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setSize(width, height);
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.05;
  renderer.domElement.style.width = "100%";
  renderer.domElement.style.height = "100%";
  renderer.domElement.style.display = "block";
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color("#07090f");

  const camera = new THREE.PerspectiveCamera(42, width / height, 0.01, 100);
  camera.position.set(1.6, 1.1, 2.1);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotateSpeed = 1.4;

  scene.add(new THREE.HemisphereLight(0x8fd8ff, 0x140b2a, 1.5));
  const key = new THREE.DirectionalLight(0xffffff, 2.4);
  key.position.set(3, 4, 2);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xa855f7, 1.5);
  rim.position.set(-3, 1, -3);
  scene.add(rim);

  const grid = new THREE.GridHelper(6, 24, 0x1b2740, 0x111a2a);
  grid.position.y = -0.7;
  scene.add(grid);

  const loader = new GLTFLoader();
  let model: THREE.Object3D | null = null;
  try {
    const gltf = await loader.loadAsync(url);
    model = gltf.scene;
    model.traverse((node) => {
      const mesh = node as THREE.Mesh;
      if (mesh.isMesh) {
        mesh.material = new THREE.MeshStandardMaterial({
          vertexColors: true,
          roughness: 0.62,
          metalness: 0.05,
          side: THREE.DoubleSide,
        });
      }
    });
    const box = new THREE.Box3().setFromObject(model);
    const sphere = box.getBoundingSphere(new THREE.Sphere());
    const center = sphere.center;
    model.position.sub(center);
    const radius = Math.max(sphere.radius, 0.1);
    const distance = radius / Math.sin((camera.fov * Math.PI) / 360) * 1.15;
    camera.position.set(distance * 0.6, distance * 0.42, distance * 0.82);
    camera.near = radius / 100;
    camera.far = radius * 100;
    camera.updateProjectionMatrix();
    controls.target.set(0, 0, 0);
    controls.minDistance = radius * 0.6;
    controls.maxDistance = radius * 12;
    grid.position.y = -radius * 1.35;
    grid.scale.setScalar(radius * 2.2);
    controls.update();
    scene.add(model);
  } catch (error) {
    renderer.dispose();
    container.replaceChildren();
    throw error instanceof Error ? error : new Error("Could not load the GLB mesh.");
  }

  let frame = 0;
  const animate = () => {
    frame = requestAnimationFrame(animate);
    controls.update();
    renderer.render(scene, camera);
  };
  animate();

  const resize = () => {
    const w = Math.max(1, container.clientWidth);
    const h = Math.max(1, container.clientHeight);
    renderer.setSize(w, h);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(container);

  return {
    reset() {
      if (!model) return;
      const box = new THREE.Box3().setFromObject(model);
      const sphere = box.getBoundingSphere(new THREE.Sphere());
      const radius = Math.max(sphere.radius, 0.1);
      const distance = (radius / Math.sin((camera.fov * Math.PI) / 360)) * 1.15;
      camera.position.set(distance * 0.6, distance * 0.42, distance * 0.82);
      controls.target.set(0, 0, 0);
      controls.autoRotate = false;
      controls.update();
    },
    setAutoRotate(enabled: boolean) {
      controls.autoRotate = enabled;
    },
    dispose() {
      cancelAnimationFrame(frame);
      observer.disconnect();
      controls.dispose();
      scene.traverse((node) => {
        const mesh = node as THREE.Mesh;
        if (mesh.geometry) mesh.geometry.dispose();
        const material = mesh.material as THREE.Material | THREE.Material[] | undefined;
        if (Array.isArray(material)) material.forEach((m) => m.dispose());
        else material?.dispose();
      });
      renderer.dispose();
      container.replaceChildren();
    },
  };
}
