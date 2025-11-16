#!/bin/bash
# Deploy xml-to-ndjson to Google Cloud Composer
#
# Usage: ./deploy.sh [composer-bucket-name]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
COMPOSER_BUCKET="${1:-}"
BINARY_PATH="target/release/xml-to-ndjson"
DAG_PATH="examples/composer_dag.py"

echo -e "${GREEN}=== XML to NDJSON Deployment ===${NC}\n"

# Check if Composer bucket is provided
if [ -z "$COMPOSER_BUCKET" ]; then
    echo -e "${RED}Error: Composer bucket name required${NC}"
    echo "Usage: ./deploy.sh gs://your-composer-bucket"
    echo ""
    echo "Example:"
    echo "  ./deploy.sh gs://europe-west2-my-composer-env-12345-bucket"
    exit 1
fi

# Remove gs:// prefix if present
COMPOSER_BUCKET="${COMPOSER_BUCKET#gs://}"

echo "Target bucket: gs://${COMPOSER_BUCKET}"
echo ""

# Check if binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo -e "${YELLOW}Binary not found. Building release...${NC}"
    cargo build --release
    echo -e "${GREEN}✓ Build complete${NC}\n"
fi

# Check if gcloud is installed
if ! command -v gsutil &> /dev/null; then
    echo -e "${RED}Error: gsutil not found${NC}"
    echo "Please install Google Cloud SDK: https://cloud.google.com/sdk/install"
    exit 1
fi

# Deploy binary
echo -e "${YELLOW}Deploying binary...${NC}"
gsutil cp "$BINARY_PATH" "gs://${COMPOSER_BUCKET}/data/bin/xml-to-ndjson"
echo -e "${GREEN}✓ Binary deployed to gs://${COMPOSER_BUCKET}/data/bin/xml-to-ndjson${NC}\n"

# Deploy DAG (optional)
read -p "Deploy example DAG? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Deploying DAG...${NC}"
    gsutil cp "$DAG_PATH" "gs://${COMPOSER_BUCKET}/dags/"
    echo -e "${GREEN}✓ DAG deployed to gs://${COMPOSER_BUCKET}/dags/${NC}"
    echo -e "${YELLOW}⚠ Remember to update PROJECT_ID and BUCKET_NAME in the DAG${NC}\n"
fi

echo -e "${GREEN}=== Deployment Complete ===${NC}"
echo ""
echo "Binary location:"
echo "  gs://${COMPOSER_BUCKET}/data/bin/xml-to-ndjson"
echo ""
echo "To use in Composer, reference the binary as:"
echo "  /home/airflow/gcs/data/bin/xml-to-ndjson"
echo ""
echo "Next steps:"
echo "  1. Update your DAG configuration (PROJECT_ID, BUCKET_NAME, etc.)"
echo "  2. Test the converter with sample data"
echo "  3. Monitor the DAG execution in Airflow UI"
