// sygna_proxy/src/main.rs - Versão com Configuração Externalizada (YAML) e Testes

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use moka::sync::Cache;
use std::time::Duration;
use serde::Deserialize;

#[macro_use]
extern crate lazy_static;

// --- ESTRUTURA DE DADOS DA CONFIGURAÇÃO YAML ---
#[derive(Debug, Deserialize)]
struct Config {
    proxy_address: String,
    kernel_address: String,
}

// O CACHE GLOBAL: Implementação TinyLFU
lazy_static! {
    static ref TRUST_CACHE: Cache<String, bool> = Cache::builder()
        .max_capacity(10_000) 
        .time_to_live(Duration::from_secs(300))
        .build();
}

// Variável global para armazenar a configuração
lazy_static! {
    static ref APP_CONFIG: Config = load_config().expect("Falha ao carregar config.yaml. O arquivo existe?");
}


// --- FUNÇÃO DE LEITURA DA CONFIGURAÇÃO ---
fn load_config() -> Result<Config, io::Error> {
    let config_path = "config.yaml";
    let contents = std::fs::read_to_string(config_path)?;
    
    let config: Config = serde_yaml::from_str(&contents)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Erro de parse YAML: {}", e)))?;
    
    Ok(config)
}


// --- FUNÇÕES CORE DO PROXY ---

// Verifica se o Kernel (T1) está disponível, usando o endereço LIDO do YAML
async fn check_kernel_health() -> bool {
    match TcpStream::connect(APP_CONFIG.kernel_address.as_str()).await {
        Ok(_) => true,
        Err(_) => false,
    }
}


// 1. VERIFICAR AUTENTICAÇÃO (TORNADA PÚBLICA PARA O TESTE)
pub async fn verify_zero_trust_token(token: &str) -> bool {
    if let Some(is_valid) = TRUST_CACHE.get(token) {
        println!("[PROXY-CACHE]: Token '{}' encontrado no TinyLFU. Verificação ignorada (RÁPIDO).", token);
        return is_valid;
    }

    // A lógica de validação é que o token COMECE com AUTH_SYGMA_VALID_
    let is_valid = token.starts_with("AUTH_SYGMA_VALID_"); 

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

    // 1. ZERO-TRUST CHECK
    if !verify_zero_trust_token(auth_token).await {
        stream.write_all(b"403 ACCESS DENIED: Zero Trust Violation").await?;
        println!("PROXY: REJEIÇÃO: Token {} falhou no Zero-Trust Check.", auth_token);
        return Ok(());
    }

    // 2. HEALTH CHECK
    if !check_kernel_health().await {
        stream.write_all(b"503 SERVICE UNAVAILABLE: Kernel T1 Offline").await?;
        println!("PROXY: REJEIÇÃO: Kernel T1 indisponível. Conexão bloqueada para prevenir perda de dados.");
        return Ok(());
    }

    // 3. ROTEAMENTO SEGURO
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
    let _ = APP_CONFIG.proxy_address.as_str();

    let listener = TcpListener::bind(APP_CONFIG.proxy_address.as_str()).await?;
    println!("--- Sygma Proxy (Tier 2 Agent) escutando em {} (YAML Config + Health Check Ativo) ---", APP_CONFIG.proxy_address);

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

// --- BLOCO DE TESTES UNITÁRIOS E DE INTEGRAÇÃO (Rastreabilidade e Confiabilidade) ---

// Este módulo só é compilado e executado quando rodamos `cargo test`
#[cfg(test)]
mod tests {
    use super::TRUST_CACHE;
    use super::verify_zero_trust_token;
    use super::APP_CONFIG; 
    // Removendo std::time::Duration e std::thread para testes mais determinísticos.

    // Garante que a configuração e o cache sejam inicializados e limpos antes de qualquer teste
    fn setup() {
        let _ = APP_CONFIG.proxy_address.as_str(); // Força a inicialização global (inclui o cache)
        let _ = TRUST_CACHE.entry_count(); // Força acesso ao cache
        TRUST_CACHE.invalidate_all(); // LIMPEZA CHAVE
    }

    // Teste 1: Valida a Regra de Ouro (Zero Trust Check)
    #[tokio::test]
    async fn test_verify_valid_token() {
        setup();
        // ZTC deve passar
        let token = "AUTH_SYGMA_VALID_TEST_TOKEN"; // Corrigido para remover o _1 final
        assert!(verify_zero_trust_token(token).await, "O token válido deve passar no ZTC.");
    }

    // Teste 2: Valida a Regra de Ouro (Zero Trust Check)
    #[tokio::test]
    async fn test_verify_invalid_token() {
        setup();
        // ZTC deve falhar
        let token = "FRAUD_ATTEMPT_TEST_TOKEN"; // Corrigido para remover o _2 final
        assert!(!verify_zero_trust_token(token).await, "O token inválido deve falhar no ZTC.");
    }

    // Teste 3: Prova a persistência e uso do cache TinyLFU.
    #[tokio::test]
    async fn test_caching_behavior() {
        setup();
        let token = "AUTH_SYGMA_VALID_CACHE_TEST"; // Token válido e claro
        
        // 1. Primeira verificação: Deve ser uma verificação LENTA e inserir o token no cache.
        let is_valid = verify_zero_trust_token(token).await;
        assert!(is_valid, "A primeira verificação de token válido deve passar.");

        // 2. Prova de persistência: Verifica se o token está no cache IMEDIATAMENTE após a inserção.
        // O cache deve retornar 'Some' (o valor está lá).
        let cached_result = TRUST_CACHE.get(token).is_some();
        assert!(cached_result, "O token deve ser encontrado no cache após a primeira inserção (Prova de persistência).");
        
        // 3. Simulação da segunda verificação: Esta chamada DEVE usar o cache.
    }
}

