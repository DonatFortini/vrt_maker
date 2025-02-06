import * as THREE from 'three';
import { parse } from '../node_modules/geotiff/dist-browser/geotiff.js';

let scene, camera, renderer, terrain;
const MOVE_SPEED = 2.0;
const ZOOM_SPEED = 0.1;
const ROTATION_SPEED = 0.01;

// Touch and mouse state
let isRotating = false;
let previousTouch = { x: 0, y: 0 };
let previousMouse = { x: 0, y: 0 };

async function loadTiff(url) {
    try {
        console.log('Loading TIFF from:', url);
        const response = await fetch(url);
        const arrayBuffer = await response.arrayBuffer();
        console.log('Array buffer loaded, size:', arrayBuffer.byteLength);
        
        const tiff = await parse(arrayBuffer);
        console.log('TIFF parsed:', tiff);
        
        const image = await tiff.getImage();
        console.log('Got image:', image.getWidth(), 'x', image.getHeight());
        
        const data = await image.readRasters();
        console.log('Rasters read:', data.length, 'bands');
        
        return {
            data: data[0],
            width: image.getWidth(),
            height: image.getHeight(),
            max: Math.max(...data[0]),
            min: Math.min(...data[0])
        };
    } catch (error) {
        console.error('Detailed error in loadTiff:', error);
        throw error;
    }
}

// ... rest of the code stays the same ...

function createTerrainGeometry(demData) {
    const geometry = new THREE.PlaneGeometry(
        100, 100,
        demData.width - 1, demData.height - 1
    );

    // Update vertices based on DEM data
    const vertices = geometry.attributes.position.array;
    for (let i = 0; i < vertices.length; i += 3) {
        const x = Math.floor((i / 3) % demData.width);
        const y = Math.floor((i / 3) / demData.width);
        if (y < demData.height) {
            const height = demData.data[y * demData.width + x];
            // Normalize height value
            const normalizedHeight = (height - demData.min) / (demData.max - demData.min) * 20;
            vertices[i + 2] = normalizedHeight;
        }
    }

    geometry.computeVertexNormals();
    return geometry;
}

async function init() {
    // Scene setup
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x000000);

    // Camera setup
    camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);
    camera.position.set(0, 50, 50);
    camera.lookAt(0, 0, 0);

    // Renderer setup
    renderer = new THREE.WebGLRenderer({
        canvas: document.querySelector('#terrain'),
        antialias: true
    });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(window.devicePixelRatio);

    try {
        // Load TIFF files
        console.log('Loading DEM...');
        const demData = await loadTiff('dem.tiff');
        console.log('DEM loaded:', demData);

        console.log('Loading orthophoto...');
        const orthoData = await loadTiff('orthophoto.tiff');
        console.log('Orthophoto loaded:', orthoData);

        // Create terrain geometry from DEM
        const geometry = createTerrainGeometry(demData);

        // Create texture from orthophoto
        const texture = new THREE.DataTexture(
            new Uint8Array(orthoData.data),
            orthoData.width,
            orthoData.height,
            THREE.RGBAFormat
        );
        texture.needsUpdate = true;

        // Create terrain mesh
        const material = new THREE.MeshPhongMaterial({
            map: texture,
            side: THREE.DoubleSide
        });

        terrain = new THREE.Mesh(geometry, material);
        terrain.rotation.x = -Math.PI / 2;
        scene.add(terrain);
        
        document.getElementById('loading').style.display = 'none';
    } catch (error) {
        console.error('Error loading TIFF files:', error);
        document.getElementById('loading').textContent = 'Error loading terrain data';
    }

    // Lighting
    const directionalLight = new THREE.DirectionalLight(0xffffff, 1);
    directionalLight.position.set(1, 1, 1);
    scene.add(directionalLight);

    const ambientLight = new THREE.AmbientLight(0x404040, 0.5);
    scene.add(ambientLight);

    setupControls();
    animate();
}

function setupControls() {
    const canvas = document.querySelector('#terrain');

    // Mouse rotation controls
    canvas.addEventListener('mousedown', startRotation);
    canvas.addEventListener('mousemove', handleRotation);
    canvas.addEventListener('mouseup', stopRotation);
    canvas.addEventListener('mouseleave', stopRotation);

    // Touch rotation controls
    canvas.addEventListener('touchstart', startTouchRotation);
    canvas.addEventListener('touchmove', handleTouchRotation);
    canvas.addEventListener('touchend', stopRotation);

    // Zoom control
    window.addEventListener('wheel', (event) => {
        const zoomAmount = event.deltaY * ZOOM_SPEED;
        camera.position.multiplyScalar(1 + zoomAmount * 0.001);
    });

    // Navigation controls
    const navButtons = {
        'up': () => moveCamera('forward'),
        'down': () => moveCamera('backward'),
        'left': () => moveCamera('left'),
        'right': () => moveCamera('right'),
        'center': () => resetCamera()
    };

    for (const [id, handler] of Object.entries(navButtons)) {
        const button = document.getElementById(id);
        button.addEventListener('mousedown', startMove(handler));
        button.addEventListener('mouseup', stopMove);
        button.addEventListener('mouseleave', stopMove);
        button.addEventListener('touchstart', (e) => {
            e.preventDefault();
            startMove(handler)();
        });
        button.addEventListener('touchend', stopMove);
    }
}

// ... rest of the control functions remain the same ...

function startRotation(event) {
    if (!event.target.closest('#controls')) {
        isRotating = true;
        previousMouse = {
            x: event.clientX,
            y: event.clientY
        };
    }
}

function startTouchRotation(event) {
    if (!event.target.closest('#controls')) {
        isRotating = true;
        previousTouch = {
            x: event.touches[0].clientX,
            y: event.touches[0].clientY
        };
    }
}

function handleRotation(event) {
    if (!isRotating) return;

    const deltaX = event.clientX - previousMouse.x;
    const deltaY = event.clientY - previousMouse.y;
    
    rotateCamera(deltaX, deltaY);
    
    previousMouse = {
        x: event.clientX,
        y: event.clientY
    };
}

function handleTouchRotation(event) {
    if (!isRotating) return;

    const deltaX = event.touches[0].clientX - previousTouch.x;
    const deltaY = event.touches[0].clientY - previousTouch.y;
    
    rotateCamera(deltaX, deltaY);
    
    previousTouch = {
        x: event.touches[0].clientX,
        y: event.touches[0].clientY
    };
}

function rotateCamera(deltaX, deltaY) {
    const cameraDirection = new THREE.Vector3();
    camera.getWorldDirection(cameraDirection);

    // Rotate around Y axis (left/right)
    const rotationMatrixY = new THREE.Matrix4();
    rotationMatrixY.makeRotationY(-deltaX * ROTATION_SPEED);
    camera.position.applyMatrix4(rotationMatrixY);

    // Rotate around the right vector (up/down)
    const right = new THREE.Vector3();
    right.crossVectors(camera.up, cameraDirection).normalize();
    const rotationMatrixX = new THREE.Matrix4();
    rotationMatrixX.makeRotationAxis(right, -deltaY * ROTATION_SPEED);
    camera.position.applyMatrix4(rotationMatrixX);

    camera.lookAt(0, 0, 0);
}

function stopRotation() {
    isRotating = false;
}

let moveInterval = null;

function startMove(moveFunc) {
    return () => {
        if (moveInterval) clearInterval(moveInterval);
        moveFunc();
        moveInterval = setInterval(moveFunc, 50);
    };
}

function stopMove() {
    if (moveInterval) {
        clearInterval(moveInterval);
        moveInterval = null;
    }
}

function moveCamera(direction) {
    const moveVector = new THREE.Vector3();
    const cameraDirection = new THREE.Vector3();
    camera.getWorldDirection(cameraDirection);

    switch (direction) {
        case 'forward':
            moveVector.copy(cameraDirection).multiplyScalar(MOVE_SPEED);
            break;
        case 'backward':
            moveVector.copy(cameraDirection).multiplyScalar(-MOVE_SPEED);
            break;
        case 'left':
            moveVector.crossVectors(new THREE.Vector3(0, 1, 0), cameraDirection).normalize().multiplyScalar(MOVE_SPEED);
            break;
        case 'right':
            moveVector.crossVectors(cameraDirection, new THREE.Vector3(0, 1, 0)).normalize().multiplyScalar(MOVE_SPEED);
            break;
    }

    camera.position.add(moveVector);
    camera.lookAt(0, 0, 0);
}

function resetCamera() {
    camera.position.set(0, 50, 50);
    camera.lookAt(0, 0, 0);
}

function animate() {
    requestAnimationFrame(animate);
    renderer.render(scene, camera);
}

function onWindowResize() {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
}

window.addEventListener('resize', onWindowResize);
document.addEventListener('DOMContentLoaded', init)