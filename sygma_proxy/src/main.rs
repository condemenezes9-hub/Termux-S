// sygna_proxy/src/main.rs - Versão com Cache TinyLFU e Health Check

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use moka::sync::Cache;
use std::time::Duration;

#[macro_use]
extern crate lazy_static;

// ENDEREÇOS FIXOS DO SISTEMA
const PROXY_ADDRESS: &str = "127.0.0.1:7878";
const KERNEL_ADDRESS: &str = "127.0.0.1:8080"; 

// O CACHE GLOBAL: Implementação TinyLFU
lazy_static! {
    static ref TRUST_CACHE: Cache<String, bool> = Cache::builder()
        .max_capacity(10_000) 
        .time_to_live(Duration::from_secs(300))
        .build();
}

// --- FUNÇÕES CORE DO PROXY ---

// FUNÇÃO NOVA: Verifica se o Kernel (T1) está disponível
async fn check_kernel_health() -> bool {
    // Tenta conectar ao endereço do Kernel (porta 8080 simulada)
    match TcpStream::connect(KERNEL_ADDRESS).await {
        Ok(_) => {
            // Conexão bem-sucedida. Kernel está "saudável"
            true
        }
        Err(_) => {
            // Falha ao conectar. Kernel está inativo/morto.
            false
        }
    }
}


// 1. VERIFICAR AUTENTICAÇÃO (O Zero-Trust Check com Caching)
async fn verify_zero_trust_token(token: &str) -> bool {
    // 1.1 TENTAR OBTER DO CACHE (TinyLFU)
    if let Some(is_valid) = TRUST_CACHE.get(token) {
        println!("[PROXY-CACHE]: Token '{}' encontrado no TinyLFU. Verificação ignorada (RÁPIDO).", token);
        return is_valid;
    }

    // 1.2 SE NÃO ESTÁ NO CACHE, EXECUTAR A LÓGICA DE VERIFICAÇÃO LENTA
    let is_valid = token.starts_with("AUTH_SYGMA_VALID_"); 

    // 1.3 ARMAZENAR NO CACHE
    if is_valid {
        TRUST_CACHE.insert(token.to_string(), true);
        println!("[PROXY-CACHE]: Token '{}' verificado e adicionado ao TinyLFU.", token);
    }
    
    is_valid
}

// 2. ROTEAMENTO SEGURO DE CONEXÕES 
async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    let request_data = String::from_utf8_lossy(&buffer[..n]);
    let parts: Vec<&str> = request_data.split('|').collect();
    
    if parts.len() < 2 {
        stream.write_all(b"400 ERROR: Invalid Sygma Request Format").await?;
        return Ok(());
    }

    let auth_token = parts[0].trim();
    let kernel_payload = parts[1].trim();

    // --- 1. EXECUTAR O ZERO-TRUST CHECK ---
    if !verify_zero_trust_token(auth_token).await {
        stream.write_all(b"403 ACCESS DENIED: Zero Trust Violation").await?;
        println!("PROXY: REJEIÇÃO: Token {} falhou no Zero-Trust Check.", auth_token);
        return Ok(());
    }

    // --- 2. EXECUTAR O HEALTH CHECK ANTES DE CONECTAR ---
    // Apenas roteia se o Zero-Trust for válido E o Kernel estiver saudável.
    if !check_kernel_health().await {
        stream.write_all(b"503 SERVICE UNAVAILABLE: Kernel T1 Offline").await?;
        println!("PROXY: REJEIÇÃO: Kernel T1 indisponível. Conexão bloqueada para prevenir perda de dados.");
        return Ok(());
    }

    // --- 3. ROTEAMENTO SEGURO (SIMULAÇÃO) ---
    println!("PROXY: Roteando payload para o Kernel (Health Check OK)...");
    
    let kernel_response = format!("200 OK: Payload {} submetido ao Kernel T1. Aguardando Settlement.", kernel_payload);
    stream.write_all(kernel_response.as_bytes()).await?;

    Ok(())
}

// ----------------------------------------------------------------------
// FUNÇÃO PRINCIPAL: Inicia o Listener Assíncrono
// ----------------------------------------------------------------------
#[tokio::main]
async fn main() -> io::Result<()> {
    let _ = TRUST_CACHE.entry_count(); 
    
    let listener = TcpListener::bind(PROXY_ADDRESS).await?;
    // MENSAGEM CHAVE DE INICIALIZAÇÃO ALTERADA AQUI
    println!("--- Sygma Proxy (Tier 2 Agent) escutando em {} (TinyLFU + Health Check Ativo) ---", PROXY_ADDRESS);

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("PROXY: Conexão recebida de {}", addr);
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("PROXY ERROR: Falha ao lidar com a conexão: {}", e);
            }
        });
    }
}

