import { serve, file } from "bun";

const server = serve({
    port: 3000,
    fetch(req) {
        const url = new URL(req.url);
        let path = url.pathname;
        
        // Serve index.html for root path
        if (path === "/") {
            path = "/index.html";
        }

        try {
            // Set correct MIME types for TIFF files
            const headers = new Headers();
            if (path.endsWith('.tiff') || path.endsWith('.tif')) {
                headers.set('Content-Type', 'image/tiff');
            }

            // Remove leading slash and serve file from current directory
            const filePath = path.startsWith('/') ? path.slice(1) : path;
            const fileResponse = new Response(file(filePath));
            
            // Copy headers to the response
            headers.forEach((value, key) => fileResponse.headers.set(key, value));
            
            return fileResponse;
        } catch (error) {
            console.error('Error serving file:', path, error);
            return new Response('File not found', { status: 404 });
        }
    },
});

console.log(`Server running at http://localhost:${server.port}`);