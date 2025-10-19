// sygma_client/src/main.rs - Gerador de Payloads Estruturados (Tier 3)

use tokio::net::TcpStream;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use rand::Rng;

const PROXY_ADDRESS: &str = "127.0.0.1:7878";
const VALID_TOKEN_PREFIX: &str = "AUTH_SYGMA_VALID_";
const INVALID_TOKEN_PREFIX: &str = "FRAUD_ATTEMPT_";

// Geração do Payload ZKP Simulado (O "JSON de Intenção" que o LLM gera)
fn generate_zkp_payload() -> String {
    let mut rng = rand::thread_rng();
    let sender_id: u64 = rng.gen();
    let receiver_id: u64 = rng.gen();
    let amount: u64 = rng.gen_range(100..10000);
    
    // O payload simulado (hash do comando)
    format!("ZKP_HASH_S:{}_R:{}_A:{}", sender_id, receiver_id, amount)
}

// Envio do Comando Estruturado para o Proxy
async fn send_command(token: &str, payload: &str) -> io::Result<()> {
    let command = format!("{}|{}", token, payload);
    
    println!("CLIENT: Tentando conexão com Proxy em {}", PROXY_ADDRESS);
    
    match TcpStream::connect(PROXY_ADDRESS).await {
        Ok(mut stream) => {
            // 1. Envio do Comando
            stream.write_all(command.as_bytes()).await?;
            
            // 2. Leitura da Resposta do Proxy
            let mut response = vec![0; 1024];
            let n = stream.read(&mut response).await?;
            let response_str = String::from_utf8_lossy(&response[..n]);
            
            println!("\nCLIENT: Resposta do Proxy:");
            println!("--------------------------------------------------");
            println!("{}", response_str.trim());
            println!("--------------------------------------------------");
        }
        Err(e) => {
            eprintln!("\nCLIENT ERROR: Falha ao conectar ao Proxy: {}. O Proxy está rodando?", e);
        }
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("--- Sygma Client (Tier 3) Iniciado ---");

    // --- TESTE 1: Transação Válida ---
    let valid_token = format!("{}{}", VALID_TOKEN_PREFIX, rand::thread_rng().gen::<u64>());
    let valid_payload = generate_zkp_payload();
    println!("\n[TESTE 1: VALIDO] (Token: {})", valid_token);
    send_command(&valid_token, &valid_payload).await?;

    // --- TESTE 2: Transação Inválida/Fraude ---
    let invalid_token = format!("{}{}", INVALID_TOKEN_PREFIX, rand::thread_rng().gen::<u64>());
    let invalid_payload = generate_zkp_payload();
    println!("\n[TESTE 2: FRAUDE] (Token: {})", invalid_token);
    send_command(&invalid_token, &invalid_payload).await?;

    Ok(())
}

