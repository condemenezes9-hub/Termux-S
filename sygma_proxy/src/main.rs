// sygna_proxy/src/main.rs

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

// ENDEREÇOS FIXOS DO SISTEMA
const PROXY_ADDRESS: &str = "127.0.0.1:7878";
// O Kernel (Tier 1) deve rodar nesta porta, mas vamos apenas simular a comunicação
const KERNEL_ADDRESS: &str = "127.0.0.1:8080"; 

// --- FUNÇÕES CORE DO PROXY ---

// 1. VERIFICAR AUTENTICAÇÃO (O Zero-Trust Check)
async fn verify_zero_trust_token(token: &str) -> bool {
    // Regra de Logica: Apenas tokens que começam com "AUTH_SYGMA" são válidos.
    token.starts_with("AUTH_SYGMA_VALID_") 
}

// 2. ROTEAMENTO SEGURO DE CONEXÕES
async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;

    let request_data = String::from_utf8_lossy(&buffer[..n]);
    // Esperamos o formato: [TOKEN]| [PAYLOAD_ZKP]
    let parts: Vec<&str> = request_data.split('|').collect();

    if parts.len() < 2 {
        stream.write_all(b"400 ERROR: Invalid Sygma Request Format").await?;
        return Ok(());
    }

    let auth_token = parts[0].trim();
    let kernel_payload = parts[1].trim();

    // --- EXECUTAR O ZERO-TRUST CHECK ---
    if !verify_zero_trust_token(auth_token).await {
        // Rejeição Zero-Trust: Conexão encerrada.
        stream.write_all(b"403 ACCESS DENIED: Zero Trust Violation").await?;
        println!("PROXY: REJEIÇÃO: Token {} falhou no Zero-Trust Check.", auth_token);
        return Ok(());
    }

    println!("PROXY: Token Válido. Roteando payload para o Kernel...");

    // --- SIMULAÇÃO DE COMUNICAÇÃO COM O KERNEL (Tier 1) ---
    // A rigor, conectaríamos ao binário do Kernel na porta 8080, 
    // mas aqui vamos apenas simular a resposta do Kernel para evitar complexidade de sockets.

    let kernel_response = format!("200 OK: Payload {} submetido ao Kernel T1. Aguardando Settlement.", kernel_payload);
    stream.write_all(kernel_response.as_bytes()).await?;

    Ok(())
}

// ----------------------------------------------------------------------
// FUNÇÃO PRINCIPAL: Inicia o Listener Assíncrono
// ----------------------------------------------------------------------
#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind(PROXY_ADDRESS).await?;
    println!("--- Sygma Proxy (Tier 2 Agent) escutando em {} ---", PROXY_ADDRESS);

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("PROXY: Conexão recebida de {}", addr);

        // Spawna uma nova 'thread' assíncrona para lidar com a conexão (Alta Concorrência)
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("PROXY ERROR: Falha ao lidar com a conexão: {}", e);
            }
        });
    }
}

