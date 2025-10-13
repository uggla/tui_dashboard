use crate::{SncfAPIError, fake};
use reqwest::Client;
use serde::de::DeserializeOwned;

pub trait HTTPClient {
    fn get<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        username: &str,
        password: Option<&str>,
    ) -> impl std::future::Future<Output = Result<T, SncfAPIError>> + Send;
}

#[derive(Debug)]
pub struct ReqwestClient(Client);

#[derive(Debug)]
pub(crate) struct FakeClient;

#[allow(dead_code)]
impl ReqwestClient {
    pub fn new() -> Self {
        Self(reqwest::Client::new())
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HTTPClient for ReqwestClient {
    async fn get<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        username: &str,
        password: Option<&str>,
    ) -> Result<T, SncfAPIError> {
        let res = self
            .0
            .get(url)
            .basic_auth(username, password)
            .send()
            .await?
            .json::<T>()
            .await?;
        Ok(res)
    }
}

#[allow(dead_code)]
impl FakeClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FakeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HTTPClient for FakeClient {
    async fn get<T: DeserializeOwned + Send>(
        &self,
        url: &str,
        _username: &str,
        _password: Option<&str>,
    ) -> Result<T, SncfAPIError> {
        match url {
            "https://api.sncf.com/v1/coverage/sncf/places?q=Grenoble" => {
                Ok(serde_json::from_str(&fake::places()).unwrap())
            }

            _ => Ok(serde_json::from_str("{}").unwrap()),
        }
    }
}

// --- Test Module ---
#[cfg(test)]
mod tests {

    use crate::PlacesResponse;

    use super::*;

    #[tokio::test]
    async fn test_get_invalid_url_error() {
        let client = ReqwestClient::new();
        let invalid_url = "this is not a valid url";

        let result: Result<PlacesResponse, SncfAPIError> =
            client.get(invalid_url, "user", None).await;

        // assert that the result is an err
        assert!(result.is_err());

        let error = format!("{:?}", result.unwrap_err());
        assert_eq!(
            "HttpRequest(reqwest::Error { kind: Builder, source: RelativeUrlWithoutBase })",
            &error
        );
    }

    #[tokio::test]
    async fn test_get_url_not_exist_error() {
        let client = ReqwestClient::new();
        let invalid_url = "http://thisdomaindoesnotexist";

        let result: Result<PlacesResponse, SncfAPIError> =
            client.get(invalid_url, "user", None).await;

        // assert that the result is an err
        assert!(result.is_err());

        let error = format!("{:?}", result.unwrap_err());
        assert_eq!(
            "HttpRequest(reqwest::Error { kind: Request, url: \"http://thisdomaindoesnotexist/\", source: hyper_util::client::legacy::Error(Connect, ConnectError(\"dns error\", Custom { kind: Uncategorized, error: \"failed to lookup address information: Name or service not known\" })) })",
            &error
        );
    }

    #[tokio::test]
    async fn test_fake_client() {
        let client = FakeClient::new();
        let url = "https://api.sncf.com/v1/coverage/sncf/places?q=Grenoble";

        let result: Result<PlacesResponse, SncfAPIError> = client.get(url, "user", None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().places.len(), 9);
    }
}
