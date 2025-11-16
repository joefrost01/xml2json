"""
Example Airflow DAG for running xml-to-ndjson converter in Cloud Composer.

This DAG demonstrates how to:
1. Convert XML files from a GCS bucket to NDJSON
2. Load the NDJSON into BigQuery
3. Clean up intermediate files
"""

from airflow import DAG
from airflow.operators.bash import BashOperator
from airflow.providers.google.cloud.operators.bigquery import (
    BigQueryCreateEmptyDatasetOperator,
    BigQueryCreateEmptyTableOperator,
)
from airflow.providers.google.cloud.transfers.gcs_to_bigquery import (
    GCSToBigQueryOperator,
)
from datetime import datetime, timedelta

# Configuration
PROJECT_ID = "your-project-id"
BUCKET_NAME = "your-bucket-name"
DATASET_NAME = "trades"
TABLE_NAME = "daily_trades"
CONVERTER_PATH = "/home/airflow/gcs/data/bin/xml-to-ndjson"

default_args = {
    'owner': 'data-engineering',
    'depends_on_past': False,
    'start_date': datetime(2024, 1, 1),
    'email_on_failure': True,
    'email_on_retry': False,
    'retries': 2,
    'retry_delay': timedelta(minutes=5),
}

dag = DAG(
    'xml_to_ndjson_to_bigquery',
    default_args=default_args,
    description='Convert XML trade messages to NDJSON and load into BigQuery',
    schedule_interval='@daily',
    catchup=False,
    tags=['trades', 'conversion', 'bigquery'],
)

# Task 1: Convert XML to NDJSON
convert_xml = BashOperator(
    task_id='convert_xml_to_ndjson',
    bash_command=f"""
        {CONVERTER_PATH} \
          --source gs://{BUCKET_NAME}/kafka-sink/trades/{{{{ ds }}}} \
          --destination gs://{BUCKET_NAME}/processed/ndjson/{{{{ ds }}}} \
          --message-element message
    """,
    dag=dag,
)

# Task 2: Load NDJSON into BigQuery
load_to_bigquery = GCSToBigQueryOperator(
    task_id='load_ndjson_to_bigquery',
    bucket=BUCKET_NAME,
    source_objects=[f'processed/ndjson/{{{{ ds }}}}/*.ndjson'],
    destination_project_dataset_table=f'{PROJECT_ID}.{DATASET_NAME}.{TABLE_NAME}',
    source_format='NEWLINE_DELIMITED_JSON',
    write_disposition='WRITE_APPEND',
    autodetect=True,
    dag=dag,
)

# Task 3: Cleanup intermediate files (optional)
cleanup = BashOperator(
    task_id='cleanup_intermediate_files',
    bash_command=f"""
        gsutil -m rm -r gs://{BUCKET_NAME}/processed/ndjson/{{{{ ds }}}}
    """,
    dag=dag,
)

# Define task dependencies
convert_xml >> load_to_bigquery >> cleanup
